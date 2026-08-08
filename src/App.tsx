import { useEffect, useMemo, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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

export default function App() {
  const [instances, setInstances] = useState<MinecraftInstance[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [instanceId, setInstanceId] = useState("demo");
  const [instanceName, setInstanceName] = useState("Minecraft Demo");
  const [releaseChannel, setReleaseChannel] = useState<"latest" | "compatible">("latest");
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [launchProgress, setLaunchProgress] = useState<MinecraftLaunchProgress | null>(null);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [runningIds, setRunningIds] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<"install" | "launch" | "stop" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showCreator, setShowCreator] = useState(false);

  const selected = useMemo(
    () => instances.find((instance) => instance.id === selectedId) ?? null,
    [instances, selectedId],
  );
  const isRunning = selected ? runningIds.has(selected.id) : false;
  const visibleLogs = selected ? logs.filter((line) => line.instanceId === selected.id) : [];
  const progressPercent = progress
    ? progress.total === 0
      ? 0
      : Math.round((progress.completed / progress.total) * 100)
    : 0;

  const refreshInstances = async () => {
    const found = await invoke<MinecraftInstance[]>("list_minecraft_instances");
    setInstances(found);
    setSelectedId((current) => {
      if (found.some((instance) => instance.id === current)) return current;
      return found[0]?.id ?? "";
    });
  };

  useEffect(() => {
    if (!hasTauriRuntime()) return;

    void refreshInstances().catch((cause) => setError(String(cause)));
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

  const install = async () => {
    setError(null);
    setProgress({ stage: "metadata", completed: 0, total: 1, message: "準備中" });
    setBusy("install");

    try {
      const installed = await invoke<MinecraftInstance>("install_sandbox_demo_instance", {
        instanceId,
        name: instanceName,
        releaseChannel,
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
          <button
            className="toolbar-button accent"
            onClick={() => setShowCreator(true)}
            type="button"
          >
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
        <div className="account-chip">
          <span className="account-avatar">M</span>
          <span>
            <strong>オフライン</strong>
            <small>Demo profile</small>
          </span>
          <span className="chevron">⌄</span>
        </div>
      </header>

      <section className="workspace">
        <nav className="rail" aria-label="メインメニュー">
          <button className="rail-button active" type="button">
            <span>▦</span>ライブラリ
          </button>
          <button className="rail-button" type="button">
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
          <button className="rail-button" type="button">
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
              <button className="active" type="button" aria-label="グリッド表示">
                ▦
              </button>
              <button type="button" aria-label="リスト表示">
                ☷
              </button>
            </div>
          </div>

          <div className="instance-grid" aria-label="Minecraftインスタンス">
            {instances.length === 0 && (
              <button className="empty-library" onClick={() => setShowCreator(true)} type="button">
                <span className="empty-cube">＋</span>
                <strong>最初のインスタンスを作成</strong>
                <small>最新版または互換版のMinecraftを追加できます</small>
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
                <span className="tile-menu" aria-hidden="true">
                  •••
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
              className="icon-button"
              disabled={!selected}
              type="button"
              aria-label="インスタンス設定"
            >
              ⚙
            </button>
            <button
              className="icon-button"
              disabled={!selected}
              type="button"
              aria-label="その他の操作"
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
            <div className="log-output" aria-live="polite">
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
            <form
              className="creator-modal"
              onSubmit={(event) => {
                event.preventDefault();
                void install();
              }}
            >
              <div className="modal-heading">
                <div>
                  <p className="eyebrow">NEW INSTANCE</p>
                  <h2>インスタンスを追加</h2>
                </div>
                <button
                  onClick={() => setShowCreator(false)}
                  disabled={busy !== null}
                  type="button"
                  aria-label="閉じる"
                >
                  ×
                </button>
              </div>
              <p className="modal-copy">独立したMinecraft環境をAppContainer内に作成します。</p>
              <label>
                バージョン
                <select
                  value={releaseChannel}
                  onChange={(event) =>
                    setReleaseChannel(event.target.value as "latest" | "compatible")
                  }
                >
                  <option value="latest">最新版（推奨）</option>
                  <option value="compatible">互換版 1.12.2</option>
                </select>
              </label>
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
                  onClick={() => setShowCreator(false)}
                  disabled={busy !== null}
                  type="button"
                >
                  キャンセル
                </button>
                <button className="primary-button" disabled={busy !== null} type="submit">
                  {busy === "install" ? "インストール中…" : "作成する"}
                </button>
              </div>
            </form>
          </div>
        )}
      </section>
    </main>
  );
}
