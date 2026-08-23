import { useEffect, useMemo, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import "./App.css";

type MinecraftInstance = {
  id: string;
  name: string;
  versionId: string;
  javaPath: string;
  gameDirectory: string;
  demo: boolean;
  sandboxed: boolean;
};

type MinecraftVersion = {
  id: string;
  versionType: "release" | "snapshot" | "old_beta" | "old_alpha" | string;
  releaseTime: string;
};

type MinecraftVersionCatalog = {
  latest: {
    release: string;
    snapshot: string;
  };
  versions: MinecraftVersion[];
};

type InstallProgress = {
  stage: string;
  completed: number;
  total: number;
  message: string;
};

type MinecraftLogEvent = {
  instanceId: string;
  stream: string;
  line: string;
};

type MinecraftStatusEvent = {
  instanceId: string;
  status: "running" | "stopped";
  exitCode: number | null;
};

type MinecraftLaunchProgress = {
  instanceId: string;
  stage: string;
  message: string;
};

type MicrosoftAuthStatus = {
  configured: boolean;
  authorized: boolean;
};

type MicrosoftSignInChallenge = {
  sessionId: string;
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
};

type MicrosoftSignInPoll = {
  status: "pending" | "authorized";
  retryAfter: number | null;
};

type LogLine = MinecraftLogEvent & { id: number };

const stageLabels: Record<string, string> = {
  metadata: "バージョン情報",
  client: "Minecraft本体",
  libraries: "ライブラリ",
  "assets-index": "アセット一覧",
  assets: "ゲーム素材",
  runtime: "隔離用Java",
  complete: "完了",
};

function hasTauriRuntime() {
  const internals = (
    window as typeof window & {
      __TAURI_INTERNALS__?: { invoke?: unknown; transformCallback?: unknown };
    }
  ).__TAURI_INTERNALS__;

  return (
    isTauri() &&
    typeof internals?.invoke === "function" &&
    typeof internals.transformCallback === "function"
  );
}

function keepFocusInsideDialog(event: KeyboardEvent, dialog: HTMLDialogElement | null) {
  if (event.key !== "Tab") return;

  const focusable = Array.from(
    dialog?.querySelectorAll<HTMLElement>(
      'button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ) ?? [],
  );
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (!first || !last) return;

  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

export default function App() {
  const [instances, setInstances] = useState<MinecraftInstance[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [instanceId, setInstanceId] = useState("minecraft");
  const [instanceName, setInstanceName] = useState("Minecraft");
  const [versionCatalog, setVersionCatalog] = useState<MinecraftVersionCatalog | null>(null);
  const [selectedVersionId, setSelectedVersionId] = useState("");
  const [versionQuery, setVersionQuery] = useState("");
  const [versionsLoading, setVersionsLoading] = useState(false);
  const [gameMode, setGameMode] = useState<"offline" | "demo">("offline");
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [launchProgress, setLaunchProgress] = useState<MinecraftLaunchProgress | null>(null);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [runningIds, setRunningIds] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<"install" | "launch" | "stop" | "rename" | "delete" | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);
  const [showCreator, setShowCreator] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [settingsName, setSettingsName] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [authStatus, setAuthStatus] = useState<MicrosoftAuthStatus>({
    configured: false,
    authorized: false,
  });
  const [showAuth, setShowAuth] = useState(false);
  const [authChallenge, setAuthChallenge] = useState<MicrosoftSignInChallenge | null>(null);
  const [authBusy, setAuthBusy] = useState<"begin" | "signout" | null>(null);
  const [authError, setAuthError] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<"grid" | "list">("grid");
  const creatorDialogRef = useRef<HTMLDialogElement>(null);
  const creatorSearchRef = useRef<HTMLInputElement>(null);
  const creatorPreviousFocusRef = useRef<HTMLElement | null>(null);
  const settingsDialogRef = useRef<HTMLDialogElement>(null);
  const settingsNameRef = useRef<HTMLInputElement>(null);
  const settingsPreviousFocusRef = useRef<HTMLElement | null>(null);
  const authDialogRef = useRef<HTMLDialogElement>(null);
  const authPrimaryRef = useRef<HTMLButtonElement>(null);
  const authPreviousFocusRef = useRef<HTMLElement | null>(null);

  const selected = useMemo(
    () => instances.find((instance) => instance.id === selectedId) ?? null,
    [instances, selectedId],
  );
  const isRunning = selected ? runningIds.has(selected.id) : false;
  const visibleLogs = selected ? logs.filter((line) => line.instanceId === selected.id) : [];
  const versionGroups = useMemo(() => {
    const groups = [
      { type: "release", label: "正式リリース" },
      { type: "snapshot", label: "スナップショット" },
      { type: "old_beta", label: "旧Beta" },
      { type: "old_alpha", label: "旧Alpha" },
    ];
    const query = versionQuery.trim().toLowerCase();

    return groups.map((group) => ({
      ...group,
      versions: (versionCatalog?.versions ?? []).filter(
        (version) =>
          version.versionType === group.type &&
          (!query || version.id.toLowerCase().includes(query) || version.id === selectedVersionId),
      ),
    }));
  }, [selectedVersionId, versionCatalog, versionQuery]);
  const progressPercent = progress
    ? progress.total === 0
      ? 0
      : Math.round((progress.completed / progress.total) * 100)
    : 0;

  const openCreator = () => {
    creatorPreviousFocusRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setShowCreator(true);
  };

  const closeCreator = () => {
    if (busy === null) setShowCreator(false);
  };

  const openSettings = () => {
    if (!selected) return;
    settingsPreviousFocusRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setSettingsName(selected.name);
    setConfirmDelete(false);
    setError(null);
    setShowSettings(true);
  };

  const closeSettings = () => {
    if (busy === null) {
      setConfirmDelete(false);
      setShowSettings(false);
    }
  };

  const openAuth = () => {
    authPreviousFocusRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setAuthError(null);
    setShowAuth(true);
  };

  const closeAuth = () => {
    if (authBusy === null) setShowAuth(false);
  };

  const refreshInstances = async () => {
    const found = await invoke<MinecraftInstance[]>("list_minecraft_instances");
    setInstances(found);
    setSelectedId((current) => {
      if (found.some((instance) => instance.id === current)) return current;
      return found[0]?.id ?? "";
    });
  };

  const refreshVersions = async () => {
    setVersionsLoading(true);
    try {
      const catalog = await invoke<MinecraftVersionCatalog>("list_minecraft_versions");
      setVersionCatalog(catalog);
      setSelectedVersionId((current) =>
        catalog.versions.some((version) => version.id === current)
          ? current
          : catalog.latest.release,
      );
    } finally {
      setVersionsLoading(false);
    }
  };

  const refreshAuthStatus = async () => {
    setAuthStatus(await invoke<MicrosoftAuthStatus>("microsoft_auth_status"));
  };

  useEffect(() => {
    if (!hasTauriRuntime()) return;

    void refreshInstances().catch((cause) => setError(String(cause)));
    void refreshVersions().catch((cause) => setError(String(cause)));
    void refreshAuthStatus().catch((cause) => setAuthError(String(cause)));
    const unlistenProgress = listen<InstallProgress>("minecraft-install-progress", (event) => {
      setProgress(event.payload);
    });
    const unlistenLogs = listen<MinecraftLogEvent>("minecraft-log", (event) => {
      setLogs((current) => [
        ...current.slice(-999),
        { ...event.payload, id: Date.now() + Math.random() },
      ]);
    });
    const unlistenLaunchProgress = listen<MinecraftLaunchProgress>(
      "minecraft-launch-progress",
      (event) => setLaunchProgress(event.payload),
    );
    const unlistenStatus = listen<MinecraftStatusEvent>("minecraft-status", (event) => {
      setRunningIds((current) => {
        const next = new Set(current);
        if (event.payload.status === "running") next.add(event.payload.instanceId);
        else next.delete(event.payload.instanceId);
        return next;
      });

      if (event.payload.status === "stopped") {
        const suffix =
          event.payload.exitCode === null ? "" : ` (終了コード ${event.payload.exitCode})`;
        setLogs((current) => [
          ...current.slice(-999),
          {
            id: Date.now() + Math.random(),
            instanceId: event.payload.instanceId,
            stream: "launcher",
            line: `Minecraftが終了しました${suffix}`,
          },
        ]);
      }
    });

    return () => {
      void unlistenProgress.then((unlisten) => unlisten());
      void unlistenLogs.then((unlisten) => unlisten());
      void unlistenLaunchProgress.then((unlisten) => unlisten());
      void unlistenStatus.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    if (!showCreator) return;

    creatorSearchRef.current?.focus();
    return () => creatorPreviousFocusRef.current?.focus();
  }, [showCreator]);

  useEffect(() => {
    if (!showCreator) return;

    const handleCreatorKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && busy === null) {
        event.preventDefault();
        setShowCreator(false);
        return;
      }
      keepFocusInsideDialog(event, creatorDialogRef.current);
    };

    document.addEventListener("keydown", handleCreatorKeyDown);
    return () => document.removeEventListener("keydown", handleCreatorKeyDown);
  }, [busy, showCreator]);

  useEffect(() => {
    if (!showSettings) return;

    settingsNameRef.current?.focus();
    settingsNameRef.current?.select();
    return () => settingsPreviousFocusRef.current?.focus();
  }, [showSettings]);

  useEffect(() => {
    if (!showSettings) return;

    const handleSettingsKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && busy === null) {
        event.preventDefault();
        setShowSettings(false);
        return;
      }
      keepFocusInsideDialog(event, settingsDialogRef.current);
    };

    document.addEventListener("keydown", handleSettingsKeyDown);
    return () => document.removeEventListener("keydown", handleSettingsKeyDown);
  }, [busy, showSettings]);

  useEffect(() => {
    if (!showAuth) return;

    authPrimaryRef.current?.focus();
    if (document.activeElement !== authPrimaryRef.current) authDialogRef.current?.focus();
    return () => authPreviousFocusRef.current?.focus();
  }, [showAuth]);

  useEffect(() => {
    if (!showAuth) return;

    const handleAuthKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && authBusy === null) {
        event.preventDefault();
        setShowAuth(false);
        return;
      }
      keepFocusInsideDialog(event, authDialogRef.current);
    };

    document.addEventListener("keydown", handleAuthKeyDown);
    return () => document.removeEventListener("keydown", handleAuthKeyDown);
  }, [authBusy, showAuth]);

  useEffect(() => {
    if (!authChallenge || authStatus.authorized) return;

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      try {
        const result = await invoke<MicrosoftSignInPoll>("poll_microsoft_sign_in", {
          sessionId: authChallenge.sessionId,
        });
        if (cancelled) return;
        if (result.status === "authorized") {
          setAuthStatus((current) => ({ ...current, authorized: true }));
          setAuthChallenge(null);
          setAuthError(null);
          return;
        }
        timer = setTimeout(poll, (result.retryAfter ?? authChallenge.interval) * 1000);
      } catch (cause) {
        if (!cancelled) {
          setAuthError(String(cause));
          setAuthChallenge(null);
        }
      }
    };

    timer = setTimeout(poll, authChallenge.interval * 1000);
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [authChallenge, authStatus.authorized]);

  const install = async () => {
    setError(null);
    setProgress({ stage: "metadata", completed: 0, total: 1, message: "準備中" });
    setBusy("install");

    try {
      const installed = await invoke<MinecraftInstance>("install_sandbox_instance", {
        instanceId,
        name: instanceName,
        versionId: selectedVersionId,
        demo: gameMode === "demo",
      });
      await refreshInstances();
      setSelectedId(installed.id);
      setShowCreator(false);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const launch = async () => {
    if (!selected) return;
    setError(null);
    setBusy("launch");
    setLaunchProgress({
      instanceId: selected.id,
      stage: "queued",
      message: "起動処理を開始しています…",
    });
    setLogs((current) => current.filter((line) => line.instanceId !== selected.id));

    try {
      const pid = await invoke<number>("launch_minecraft_instance", {
        instanceId: selected.id,
      });
      setLogs((current) => [
        ...current,
        {
          id: Date.now(),
          instanceId: selected.id,
          stream: "launcher",
          line: `Minecraftを起動しました (PID ${pid})`,
        },
      ]);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLaunchProgress(null);
      setBusy(null);
    }
  };

  const stop = async () => {
    if (!selected) return;
    setError(null);
    setBusy("stop");

    try {
      await invoke("stop_minecraft_instance", { instanceId: selected.id });
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const renameSelected = async () => {
    if (!selected) return;
    setError(null);
    setBusy("rename");

    try {
      const renamed = await invoke<MinecraftInstance>("rename_minecraft_instance", {
        instanceId: selected.id,
        name: settingsName,
      });
      setInstances((current) =>
        current
          .map((instance) => (instance.id === renamed.id ? renamed : instance))
          .sort((left, right) => left.name.localeCompare(right.name, "ja")),
      );
      setShowSettings(false);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const deleteSelected = async () => {
    if (!selected) return;
    const deletedId = selected.id;
    setError(null);
    setBusy("delete");

    try {
      await invoke("delete_minecraft_instance", { instanceId: deletedId });
      setShowSettings(false);
      setConfirmDelete(false);
      const remaining = instances.filter((instance) => instance.id !== deletedId);
      setInstances(remaining);
      setSelectedId((selectedId) =>
        selectedId === deletedId ? (remaining[0]?.id ?? "") : selectedId,
      );
      setLogs((current) => current.filter((line) => line.instanceId !== deletedId));
      setRunningIds((current) => {
        const next = new Set(current);
        next.delete(deletedId);
        return next;
      });
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const beginMicrosoftSignIn = async () => {
    setAuthBusy("begin");
    setAuthError(null);
    try {
      const challenge = await invoke<MicrosoftSignInChallenge>("begin_microsoft_sign_in");
      setAuthChallenge(challenge);
      await openMicrosoftVerification(challenge.verificationUri);
    } catch (cause) {
      setAuthError(String(cause));
    } finally {
      setAuthBusy(null);
    }
  };

  const openMicrosoftVerification = async (verificationUri: string) => {
    try {
      await openUrl(verificationUri);
    } catch (cause) {
      setAuthError(`ブラウザーを開けませんでした。下のURLを手動で開いてください: ${cause}`);
    }
  };

  const signOutMicrosoft = async () => {
    setAuthBusy("signout");
    setAuthError(null);
    try {
      await invoke("sign_out_microsoft");
      setAuthChallenge(null);
      setAuthStatus((current) => ({ ...current, authorized: false }));
    } catch (cause) {
      setAuthError(String(cause));
    } finally {
      setAuthBusy(null);
    }
  };

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand">
          <div className="brand-mark" aria-hidden="true">
            <span />
          </div>
          <div>
            <h1>MonaLauncher</h1>
            <p>Secure Minecraft launcher</p>
          </div>
        </div>
        <div className="toolbar" aria-label="ランチャー操作">
          <button className="toolbar-button accent" onClick={openCreator} type="button">
            <span aria-hidden="true">＋</span>インスタンスを追加
          </button>
          <button
            className="toolbar-button"
            onClick={() => void refreshInstances().catch((cause) => setError(String(cause)))}
            type="button"
          >
            <span aria-hidden="true">↻</span>更新
          </button>
        </div>
        <button className="account-chip" onClick={openAuth} type="button">
          <span className="account-avatar" aria-hidden="true">
            M
          </span>
          <span>
            <strong>{authStatus.authorized ? "Microsoft認証済み" : "オフライン"}</strong>
            <small>
              {authStatus.authorized
                ? "Minecraft連携準備中"
                : authStatus.configured
                  ? "サインインできます"
                  : "認証設定が必要です"}
            </small>
          </span>
          <span className="chevron" aria-hidden="true">
            ›
          </span>
        </button>
      </header>

      <section className="workspace">
        <nav className="rail" aria-label="メインメニュー">
          <button className="rail-button active" type="button">
            <span>▦</span>ライブラリ
          </button>
          <button
            aria-label="ニュース（準備中）"
            className="rail-button"
            disabled
            title="ニュース機能は準備中です"
            type="button"
          >
            <span>◫</span>ニュース
          </button>
          <div className="rail-spacer" />
          <div className="security-pill" title="AppContainerによる隔離が有効です">
            <span className="shield">◆</span>
            <span>
              <strong>保護中</strong>
              <small>AppContainer</small>
            </span>
          </div>
          <button
            aria-label="設定（準備中）"
            className="rail-button"
            disabled
            title="設定機能は準備中です"
            type="button"
          >
            <span>⚙</span>設定
          </button>
        </nav>

        <section className="library">
          <div className="library-heading">
            <div>
              <p className="eyebrow">YOUR LIBRARY</p>
              <h2>インスタンス</h2>
              <p>{instances.length}個のMinecraft環境</p>
            </div>
            <div className="view-controls" aria-label="表示切り替え">
              <button
                aria-label="グリッド表示"
                aria-pressed={viewMode === "grid"}
                className={viewMode === "grid" ? "active" : ""}
                onClick={() => setViewMode("grid")}
                type="button"
              >
                ▦
              </button>
              <button
                aria-label="リスト表示"
                aria-pressed={viewMode === "list"}
                className={viewMode === "list" ? "active" : ""}
                onClick={() => setViewMode("list")}
                type="button"
              >
                ☷
              </button>
            </div>
          </div>

          <div
            className={`instance-grid ${viewMode === "list" ? "list-view" : ""}`}
            aria-label="Minecraftインスタンス"
          >
            {instances.length === 0 && (
              <button className="empty-library" onClick={openCreator} type="button">
                <span className="empty-cube">＋</span>
                <strong>最初のインスタンスを作成</strong>
                <small>公式の全バージョンからMinecraftを追加できます</small>
              </button>
            )}
            {instances.map((instance) => (
              <button
                className={"instance-tile " + (selectedId === instance.id ? "selected" : "")}
                key={instance.id}
                onClick={() => setSelectedId(instance.id)}
                aria-pressed={selectedId === instance.id}
                type="button"
              >
                <span className="instance-art">
                  <span className="grass-cube">{instance.name.slice(0, 1).toUpperCase()}</span>
                  {runningIds.has(instance.id) && <span className="playing-badge">PLAYING</span>}
                </span>
                <span className="tile-copy">
                  <strong>{instance.name}</strong>
                  <small>Minecraft {instance.versionId}</small>
                </span>
              </button>
            ))}
          </div>

          {progress && progress.stage !== "complete" && (
            <div className="progress-card" aria-live="polite">
              <div className="progress-icon">↓</div>
              <div className="progress-body">
                <div className="progress-copy">
                  <strong>{stageLabels[progress.stage] ?? progress.stage}</strong>
                  <span>{progress.message}</span>
                  <b>{progressPercent}%</b>
                </div>
                <div className="progress-track">
                  <div className="progress-value" style={{ width: progressPercent + "%" }} />
                </div>
              </div>
            </div>
          )}

          {error && (
            <div className="error-card" role="alert">
              <span>!</span>
              <div>
                <strong>処理を完了できませんでした</strong>
                <small>{error}</small>
              </div>
            </div>
          )}
        </section>

        <aside className="details-panel">
          <div className="details-hero">
            <div className="detail-icon">{selected?.name.slice(0, 1).toUpperCase() ?? "?"}</div>
            <div>
              <span className={"state-label " + (isRunning ? "online" : "")}>
                <span className={"status-dot " + (isRunning ? "running" : "")} />
                {isRunning ? "実行中" : "起動準備完了"}
              </span>
              <h2>{selected?.name ?? "未選択"}</h2>
              <p>
                {selected ? "Minecraft " + selected.versionId : "インスタンスを選択してください"}
              </p>
            </div>
          </div>

          <div className="launch-actions">
            {isRunning ? (
              <button
                className="danger-button"
                disabled={busy !== null}
                onClick={stop}
                type="button"
              >
                {busy === "stop" ? "停止中…" : "■ 停止"}
              </button>
            ) : (
              <button
                className="primary-button"
                disabled={!selected || busy !== null}
                onClick={launch}
                type="button"
              >
                <span aria-hidden="true">▶</span>
                {busy === "launch" ? "起動中…" : "起動"}
              </button>
            )}
            <button
              aria-label="インスタンス設定"
              className="icon-button"
              disabled={!selected || busy !== null}
              onClick={openSettings}
              type="button"
            >
              ⚙
            </button>
            <button
              aria-label="その他の操作（準備中）"
              className="icon-button"
              disabled
              title="その他の操作は準備中です"
              type="button"
            >
              •••
            </button>
          </div>

          {busy === "launch" && launchProgress?.instanceId === selected?.id && (
            <div className="launch-loading" aria-live="polite">
              <span className="loading-spinner" aria-hidden="true" />
              <div>
                <strong>起動準備中</strong>
                <small>{launchProgress?.message ?? "起動処理を開始しています…"}</small>
              </div>
            </div>
          )}

          <dl className="instance-facts">
            <div>
              <dt>実行方式</dt>
              <dd>{selected?.sandboxed ? "AppContainer" : selected ? "通常" : "—"}</dd>
            </div>
            <div>
              <dt>ゲームモード</dt>
              <dd>{selected?.demo ? "デモ" : selected ? "通常" : "—"}</dd>
            </div>
            <div>
              <dt>インスタンスID</dt>
              <dd>{selected?.id ?? "—"}</dd>
            </div>
          </dl>

          <div className="console">
            <div className="console-heading">
              <div>
                <span className="status-dot" />
                <strong>ライブコンソール</strong>
              </div>
              <button
                className="text-button"
                onClick={() =>
                  selected &&
                  setLogs((current) => current.filter((line) => line.instanceId !== selected.id))
                }
                type="button"
              >
                クリア
              </button>
            </div>
            <div className="log-output" aria-label="Minecraftログ" aria-live="off" role="log">
              {visibleLogs.length === 0 ? (
                <span className="log-placeholder">ゲームを起動するとログが表示されます。</span>
              ) : (
                visibleLogs.map((entry) => (
                  <div className={"log-line " + entry.stream} key={entry.id}>
                    <span>{entry.stream}</span>
                    <code>{entry.line}</code>
                  </div>
                ))
              )}
            </div>
          </div>

          <div className="isolation-note">
            <span>◆</span>
            <p>
              <strong>サンドボックス保護</strong>
              <small>専用SIDと最小限の権限で実行されます</small>
            </p>
          </div>
        </aside>

        {showCreator && (
          <div className="modal-backdrop">
            <dialog
              aria-describedby="creator-description"
              aria-labelledby="creator-title"
              className="creator-modal"
              open
              ref={creatorDialogRef}
            >
              <form
                className="creator-form"
                onSubmit={(event) => {
                  event.preventDefault();
                  void install();
                }}
              >
                <div className="modal-heading">
                  <div>
                    <p className="eyebrow">NEW INSTANCE</p>
                    <h2 id="creator-title">インスタンスを追加</h2>
                  </div>
                  <button
                    onClick={closeCreator}
                    disabled={busy !== null}
                    type="button"
                    aria-label="閉じる"
                  >
                    ×
                  </button>
                </div>
                <p className="modal-copy" id="creator-description">
                  独立したMinecraft環境をAppContainer内に作成します。
                </p>
                <label>
                  バージョン検索
                  <input
                    ref={creatorSearchRef}
                    value={versionQuery}
                    onChange={(event) => setVersionQuery(event.target.value)}
                    placeholder="例: 1.21、24w、beta"
                    disabled={versionsLoading}
                  />
                </label>
                <label>
                  バージョン
                  <select
                    value={selectedVersionId}
                    onChange={(event) => setSelectedVersionId(event.target.value)}
                    disabled={versionsLoading || !versionCatalog}
                    required
                  >
                    {!versionCatalog && <option value="">バージョン一覧を取得中…</option>}
                    {versionGroups.map(
                      (group) =>
                        group.versions.length > 0 && (
                          <optgroup
                            label={`${group.label} (${group.versions.length})`}
                            key={group.type}
                          >
                            {group.versions.map((version) => (
                              <option value={version.id} key={version.id}>
                                {version.id}
                                {version.id === versionCatalog?.latest.release
                                  ? " — 最新リリース"
                                  : version.id === versionCatalog?.latest.snapshot
                                    ? " — 最新スナップショット"
                                    : ` — ${version.releaseTime.slice(0, 10)}`}
                              </option>
                            ))}
                          </optgroup>
                        ),
                    )}
                  </select>
                </label>
                <div className="version-summary" aria-live="polite">
                  <span aria-hidden="true">↓</span>
                  <span>
                    {versionsLoading
                      ? "公式バージョン一覧を取得しています…"
                      : versionCatalog
                        ? `${versionCatalog.versions.length}件から選択できます`
                        : "バージョン一覧を取得できませんでした"}
                  </span>
                  {!versionsLoading && !versionCatalog && (
                    <button
                      type="button"
                      onClick={() =>
                        void refreshVersions().catch((cause) => setError(String(cause)))
                      }
                    >
                      再試行
                    </button>
                  )}
                </div>
                <label>
                  プレイモード
                  <select
                    value={gameMode}
                    onChange={(event) => setGameMode(event.target.value as "offline" | "demo")}
                  >
                    <option value="offline">通常版（オフライン）</option>
                    <option value="demo">公式デモ版</option>
                  </select>
                </label>
                <div className="mode-note">
                  <span aria-hidden="true">i</span>
                  {gameMode === "offline"
                    ? "ワールド作成とシングルプレイができます。オンライン機能にはMicrosoft認証が必要です。"
                    : "時間制限付きの公式デモワールドを起動します。"}
                </div>
                <label>
                  表示名
                  <input
                    value={instanceName}
                    onChange={(event) => setInstanceName(event.target.value)}
                    required
                  />
                </label>
                <label>
                  ID
                  <input
                    value={instanceId}
                    onChange={(event) => setInstanceId(event.target.value)}
                    pattern="[A-Za-z0-9_-]+"
                    title="英数字、_、- が使えます"
                    required
                  />
                </label>
                <div className="modal-actions">
                  <button
                    className="secondary-button"
                    onClick={closeCreator}
                    disabled={busy !== null}
                    type="button"
                  >
                    キャンセル
                  </button>
                  <button
                    className="primary-button"
                    disabled={busy !== null || !selectedVersionId}
                    type="submit"
                  >
                    {busy === "install" ? "インストール中…" : "作成する"}
                  </button>
                </div>
              </form>
            </dialog>
          </div>
        )}

        {showAuth && (
          <div className="modal-backdrop">
            <dialog
              aria-describedby="auth-description"
              aria-labelledby="auth-title"
              className="creator-modal auth-modal"
              open
              ref={authDialogRef}
              tabIndex={-1}
            >
              <div className="creator-form">
                <div className="modal-heading">
                  <div>
                    <p className="eyebrow">MICROSOFT ACCOUNT</p>
                    <h2 id="auth-title">Microsoftアカウント</h2>
                  </div>
                  <button
                    aria-label="閉じる"
                    disabled={authBusy !== null}
                    onClick={closeAuth}
                    type="button"
                  >
                    ×
                  </button>
                </div>
                <p className="modal-copy" id="auth-description">
                  認証はMicrosoftのブラウザー画面で行います。パスワードをMonaLauncherへ入力することはありません。
                </p>

                {!authStatus.configured && (
                  <div className="auth-state-card auth-state-warning">
                    <strong>開発用クライアントIDが未設定です</strong>
                    <p>
                      MONALAUNCHER_MICROSOFT_CLIENT_IDを設定してMonaLauncherを再ビルドしてください。
                    </p>
                  </div>
                )}

                {authStatus.authorized && (
                  <div className="auth-state-card auth-state-success">
                    <strong>Microsoft認証情報を安全に保存しました</strong>
                    <p>
                      更新トークンはWindows資格情報マネージャーにあります。Minecraftプロフィールとの接続は次の実装段階です。
                    </p>
                  </div>
                )}

                {authChallenge && !authStatus.authorized && (
                  <div className="device-code-panel">
                    <span>Microsoftの画面へ入力するコード</span>
                    <strong>{authChallenge.userCode}</strong>
                    <code>{authChallenge.verificationUri}</code>
                    <small>認証が完了するまで、この画面で自動的に確認します。</small>
                  </div>
                )}

                {authError && (
                  <div className="modal-inline-error" role="alert">
                    {authError}
                  </div>
                )}

                <div className="auth-security-note">
                  <span aria-hidden="true">◆</span>
                  <p>
                    <strong>トークンはReactへ渡しません</strong>
                    <small>device codeと更新トークンはRust側だけで処理されます。</small>
                  </p>
                </div>

                <div className="modal-actions auth-actions">
                  <button
                    className="secondary-button"
                    disabled={authBusy !== null}
                    onClick={closeAuth}
                    type="button"
                  >
                    閉じる
                  </button>
                  {authStatus.authorized ? (
                    <button
                      className="signout-button"
                      disabled={authBusy !== null}
                      onClick={() => void signOutMicrosoft()}
                      ref={authPrimaryRef}
                      type="button"
                    >
                      {authBusy === "signout" ? "削除中…" : "サインアウト"}
                    </button>
                  ) : authChallenge ? (
                    <button
                      className="primary-button"
                      disabled={authBusy !== null}
                      onClick={() => void openMicrosoftVerification(authChallenge.verificationUri)}
                      ref={authPrimaryRef}
                      type="button"
                    >
                      Microsoftを開く
                    </button>
                  ) : (
                    <button
                      className="primary-button"
                      disabled={!authStatus.configured || authBusy !== null}
                      onClick={() => void beginMicrosoftSignIn()}
                      ref={authPrimaryRef}
                      type="button"
                    >
                      {authBusy === "begin" ? "コードを取得中…" : "サインインを開始"}
                    </button>
                  )}
                </div>
              </div>
            </dialog>
          </div>
        )}

        {showSettings && selected && (
          <div className="modal-backdrop">
            <dialog
              aria-describedby="settings-description"
              aria-labelledby="settings-title"
              className="creator-modal settings-modal"
              open
              ref={settingsDialogRef}
            >
              <form
                className="creator-form"
                onSubmit={(event) => {
                  event.preventDefault();
                  void renameSelected();
                }}
              >
                <div className="modal-heading">
                  <div>
                    <p className="eyebrow">INSTANCE SETTINGS</p>
                    <h2 id="settings-title">インスタンス設定</h2>
                  </div>
                  <button
                    aria-label="閉じる"
                    disabled={busy !== null}
                    onClick={closeSettings}
                    type="button"
                  >
                    ×
                  </button>
                </div>
                <p className="modal-copy" id="settings-description">
                  表示名を変更できます。インスタンスIDとゲームデータは変わりません。
                </p>
                <dl className="settings-summary">
                  <div>
                    <dt>バージョン</dt>
                    <dd>Minecraft {selected.versionId}</dd>
                  </div>
                  <div>
                    <dt>インスタンスID</dt>
                    <dd>{selected.id}</dd>
                  </div>
                </dl>
                <label>
                  表示名
                  <input
                    maxLength={80}
                    onChange={(event) => setSettingsName(event.target.value)}
                    ref={settingsNameRef}
                    required
                    value={settingsName}
                  />
                </label>
                {error && (
                  <div className="modal-inline-error" role="alert">
                    {error}
                  </div>
                )}
                <section className="danger-zone" aria-labelledby="danger-zone-title">
                  <div>
                    <strong id="danger-zone-title">インスタンスを削除</strong>
                    <small>
                      ゲーム設定、ログ、スクリーンショット、ワールドをすべて削除します。
                    </small>
                  </div>
                  {!confirmDelete && (
                    <button
                      className="delete-instance-button"
                      disabled={busy !== null || isRunning}
                      onClick={() => setConfirmDelete(true)}
                      type="button"
                    >
                      削除…
                    </button>
                  )}
                </section>
                {isRunning && (
                  <p className="delete-running-note">削除する前にMinecraftを停止してください。</p>
                )}
                {confirmDelete && (
                  <div className="delete-confirmation" role="alert">
                    <strong>「{selected.name}」を完全に削除しますか？</strong>
                    <p>この操作は取り消せません。</p>
                    <div>
                      <button
                        className="secondary-button"
                        disabled={busy !== null}
                        onClick={() => setConfirmDelete(false)}
                        type="button"
                      >
                        戻る
                      </button>
                      <button
                        className="confirm-delete-button"
                        disabled={busy !== null}
                        onClick={() => void deleteSelected()}
                        type="button"
                      >
                        {busy === "delete" ? "削除中…" : "完全に削除"}
                      </button>
                    </div>
                  </div>
                )}
                <div className="modal-actions">
                  <button
                    className="secondary-button"
                    disabled={busy !== null}
                    onClick={closeSettings}
                    type="button"
                  >
                    キャンセル
                  </button>
                  <button
                    className="primary-button"
                    disabled={busy !== null || settingsName.trim().length === 0}
                    type="submit"
                  >
                    {busy === "rename" ? "保存中…" : "変更を保存"}
                  </button>
                </div>
              </form>
            </dialog>
          </div>
        )}
      </section>
    </main>
  );
}
