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
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [runningIds, setRunningIds] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<"install" | "launch" | "stop" | null>(null);
  const [error, setError] = useState<string | null>(null);

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
        <div className="brand-mark" aria-hidden="true">
          M
        </div>
        <div>
          <p className="eyebrow">MONA PROJECT</p>
          <h1>MonaLauncher</h1>
        </div>
        <div className="security-badge">
          <span className="status-dot running" />
          AppContainer対応
        </div>
      </header>

      <section className="workspace">
        <aside className="sidebar panel">
          <div className="section-heading">
            <div>
              <p className="eyebrow">INSTANCES</p>
              <h2>インスタンス</h2>
            </div>
            <span className="count">{instances.length}</span>
          </div>

          <div className="instance-list" aria-label="Minecraftインスタンス">
            {instances.length === 0 && (
              <p className="empty-state">まだありません。下のフォームからデモ版を作成できます。</p>
            )}
            {instances.map((instance) => (
              <button
                className={`instance-card ${selectedId === instance.id ? "selected" : ""}`}
                key={instance.id}
                onClick={() => setSelectedId(instance.id)}
                aria-pressed={selectedId === instance.id}
                type="button"
              >
                <span className="instance-icon">{instance.name.slice(0, 1).toUpperCase()}</span>
                <span>
                  <strong>{instance.name}</strong>
                  <small>
                    Minecraft {instance.versionId} · {instance.sandboxed ? "AppContainer" : "通常"}
                  </small>
                </span>
                {runningIds.has(instance.id) && <span className="status-dot running" />}
              </button>
            ))}
          </div>

          <form
            className="install-form"
            onSubmit={(event) => {
              event.preventDefault();
              void install();
            }}
          >
            <p className="eyebrow">NEW SANDBOX INSTANCE</p>
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
            <button className="secondary-button" disabled={busy !== null} type="submit">
              {busy === "install" ? "インストール中…" : "隔離デモ版を追加"}
            </button>
          </form>
        </aside>

        <section className="content">
          <div className="hero panel">
            <div>
              <p className="eyebrow">SELECTED INSTANCE</p>
              <h2>{selected?.name ?? "インスタンスを選択"}</h2>
              <p className="hero-copy">
                {selected
                  ? `Minecraft ${selected.versionId} を${selected.sandboxed ? "AppContainer内" : "通常プロセス"}で起動します。`
                  : "左側で新しいデモ用インスタンスを作成してください。"}
              </p>
            </div>
            <div className="launch-actions">
              <span className={`state-label ${isRunning ? "online" : ""}`}>
                <span className={`status-dot ${isRunning ? "running" : ""}`} />
                {isRunning ? "実行中" : "停止中"}
              </span>
              {isRunning ? (
                <button
                  className="danger-button"
                  disabled={busy !== null}
                  onClick={stop}
                  type="button"
                >
                  {busy === "stop" ? "停止中…" : "停止"}
                </button>
              ) : (
                <button
                  className="primary-button"
                  disabled={!selected || busy !== null}
                  onClick={launch}
                  type="button"
                >
                  {busy === "launch" ? "起動中…" : "▶ ゲームを起動"}
                </button>
              )}
            </div>
          </div>

          {progress && progress.stage !== "complete" && (
            <div className="progress-card panel" aria-live="polite">
              <div className="progress-copy">
                <strong>{stageLabels[progress.stage] ?? progress.stage}</strong>
                <span>{progress.message}</span>
                <b>{progressPercent}%</b>
              </div>
              <div className="progress-track">
                <div className="progress-value" style={{ width: `${progressPercent}%` }} />
              </div>
            </div>
          )}

          {error && (
            <div className="error-card" role="alert">
              <strong>処理を完了できませんでした</strong>
              <span>{error}</span>
            </div>
          )}

          <div className="console panel">
            <div className="console-heading">
              <div>
                <p className="eyebrow">LIVE OUTPUT</p>
                <h2>ゲームログ</h2>
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
                <span className="log-placeholder">
                  起動すると、Minecraftのログがここに表示されます。
                </span>
              ) : (
                visibleLogs.map((entry) => (
                  <div className={`log-line ${entry.stream}`} key={entry.id}>
                    <span>{entry.stream}</span>
                    <code>{entry.line}</code>
                  </div>
                ))
              )}
            </div>
          </div>

          <footer className="security-note">
            <span>SECURITY</span>
            新しいインスタンスは専用SIDと最小限のファイルACLを持つAppContainer内で起動します。
          </footer>
        </section>
      </section>
    </main>
  );
}
