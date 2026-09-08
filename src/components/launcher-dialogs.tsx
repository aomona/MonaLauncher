import { useEffect, useRef, useState } from "react";
import { Download, ExternalLink, Play, Plus, RefreshCw, Search } from "lucide-react";
import { type Launcher, type MinecraftInstance } from "../hooks/useLauncher";
import { Button, CopyButton, Dialog, Empty, ErrorMessage, Progress, Tabs } from "./ui";

const instanceTabs = [
  "Overview",
  "Log",
  "Version",
  "Mods",
  "Resource Packs",
  "Shader Packs",
  "Worlds",
  "Servers",
  "Screenshots",
  "Settings",
];
export function InstanceDialog({
  launcher: l,
  initialTab,
  rememberTab,
  scrollMemory,
  onClose,
}: {
  launcher: Launcher;
  initialTab: string;
  rememberTab: (tab: string) => void;
  scrollMemory: Record<string, number>;
  onClose: () => void;
}) {
  const instance = l.selected!;
  const [tab, setTab] = useState(initialTab);
  const [guard, setGuard] = useState(false);
  const pending = useRef<(() => void) | null>(null);
  const [confirm, setConfirm] = useState<"stop" | "delete" | "repair" | null>(null);
  const [deleteName, setDeleteName] = useState("");
  const [saved, setSaved] = useState(false);
  const body = useRef<HTMLDivElement>(null);
  const draft = l.settingsName !== instance.name;
  const request = (action: () => void) => {
    if (draft) {
      pending.current = action;
      setGuard(true);
    } else action();
  };
  const changeTab = (next: string) =>
    request(() => {
      setTab(next);
      rememberTab(next);
    });
  useEffect(() => {
    if (body.current) body.current.scrollTop = scrollMemory[`${instance.id}:${tab}`] ?? 0;
  }, [instance.id, tab, scrollMemory]);
  useEffect(() => {
    if (tab === "Mods")
      void l.refreshInstalledMods(instance.id).catch((cause) => l.setModError(String(cause)));
    // Refresh when opening this panel; mutations refresh the same list themselves.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab, instance.id]);
  const save = async () => {
    const ok = await l.renameSelected();
    if (ok) setSaved(true);
    return ok;
  };
  const label =
    l.launchProgress?.instanceId === instance.id
      ? l.launchProgress.message
      : (l.progress?.message ??
        l.currentModInstallProgress?.message ??
        (l.busy === "stop"
          ? "Stopping Minecraft…"
          : l.busy === "diagnose"
            ? "ファイルを検証しています…"
            : l.isRunning
              ? "Running"
              : ""));
  const footer = (
    <>
      <output className="footer-status min-w-0 flex-1">
        {l.progress ? (
          <Progress label={label} value={l.progress.total > 0 ? l.progressPercent : undefined} />
        ) : l.currentModInstallProgress ? (
          <Progress
            label={label}
            value={l.currentModInstallProgress.total > 0 ? l.modInstallPercent : undefined}
          />
        ) : (
          <span className={l.isRunning ? "text-small text-success-foreground" : "text-small"}>
            {label}
          </span>
        )}
        {l.error && (
          <p className="text-small text-danger-foreground">
            処理に失敗しました。本文の詳細を確認してください。
          </p>
        )}
      </output>
      <Button
        tone={l.isRunning ? "danger-outline" : "primary"}
        aria-busy={l.busy === "launch" || l.busy === "stop"}
        disabled={Boolean(l.busy) || l.modOperationActive || !instance.sandboxed}
        onClick={() => {
          if (l.isRunning) setConfirm("stop");
          else void l.launch();
        }}
      >
        {l.busy === "launch" ? (
          "Starting…"
        ) : l.busy === "stop" ? (
          "Stopping…"
        ) : l.isRunning ? (
          "Force Quit"
        ) : (
          <>
            <Play size={14} />
            Play
          </>
        )}
      </Button>
    </>
  );
  return (
    <>
      <Dialog title={instance.name} large onClose={() => request(onClose)} footer={footer}>
        <Tabs id="instance" tabs={instanceTabs} active={tab} onChange={changeTab} />
        <div
          ref={body}
          role="tabpanel"
          id="instance-panel"
          aria-labelledby={`instance-tab-${instanceTabs.indexOf(tab)}`}
          tabIndex={0}
          className={`dialog-body flex-1 ${tab === "Log" ? "flex flex-col" : ""}`}
          onScroll={(event) => {
            scrollMemory[`${instance.id}:${tab}`] = event.currentTarget.scrollTop;
          }}
        >
          <ErrorMessage>{l.error}</ErrorMessage>
          {!instance.sandboxed && (
            <ErrorMessage>
              旧形式のため起動できません。新しいインスタンスを作成してください。
            </ErrorMessage>
          )}
          {tab === "Overview" && (
            <>
              <h3 className="mb-4 text-section-title">基本情報</h3>
              <dl className="facts">
                <div>
                  <dt>Minecraft</dt>
                  <dd className="font-mono">{instance.versionId}</dd>
                </div>
                <div>
                  <dt>Mod Loader</dt>
                  <dd className="font-mono">
                    {instance.modLoader.type === "fabric"
                      ? `Fabric ${instance.modLoader.version}`
                      : "Vanilla"}
                  </dd>
                </div>
                <div>
                  <dt>Game directory</dt>
                  <dd>
                    <span className="font-mono">{instance.gameDirectory}</span>
                    <CopyButton value={instance.gameDirectory} label="ゲームディレクトリ" />
                  </dd>
                </div>
                <div>
                  <dt>実行方式</dt>
                  <dd>{instance.sandboxed ? "AppContainer · ネットワーク権限なし" : "旧形式"}</dd>
                </div>
                <div>
                  <dt>ゲームモード</dt>
                  <dd>{instance.demo ? "デモ" : "通常"}</dd>
                </div>
              </dl>
              <section className="mt-8">
                <div className="flex flex-wrap items-center justify-between gap-4">
                  <h3 className="text-section-title">インスタンスの診断</h3>
                  <Button disabled={Boolean(l.busy)} onClick={() => void l.diagnoseSelected()}>
                    <RefreshCw size={16} />
                    {l.busy === "diagnose" ? "診断中…" : "診断する"}
                  </Button>
                </div>
                <p className="mt-2 text-text-secondary">管理対象ファイルと隔離設定を確認します。</p>
                {l.diagnosis?.instanceId === instance.id && (
                  <div className="mt-4">
                    <p className="text-small">
                      {l.diagnosis.checkedFiles}ファイル検証 · {l.diagnosis.issueCount}件の問題
                    </p>
                    <ul>
                      {l.diagnosis.checks.map((check) => (
                        <li className="border-b border-border-subtle py-4" key={check.id}>
                          <p
                            className={
                              check.status === "ok"
                                ? "text-success-foreground"
                                : check.status === "error"
                                  ? "text-danger-foreground"
                                  : "text-warning-foreground"
                            }
                          >
                            {check.status === "ok"
                              ? "正常"
                              : check.status === "error"
                                ? "Error"
                                : "Warning"}{" "}
                            · {check.label}
                          </p>
                          <p className="mt-1 wrap-anywhere text-small text-text-secondary">
                            {check.detail}
                          </p>
                        </li>
                      ))}
                    </ul>
                    {l.diagnosis.repairableCount > 0 && (
                      <Button
                        className="mt-4"
                        disabled={Boolean(l.busy) || l.isRunning}
                        onClick={() => setConfirm("repair")}
                      >
                        管理対象ファイルを修復
                      </Button>
                    )}
                  </div>
                )}
              </section>
            </>
          )}
          {tab === "Log" && <LogPanel launcher={l} />}
          {tab === "Version" && (
            <>
              <h3 className="mb-4 text-section-title">Component versions</h3>
              <dl className="facts">
                <div>
                  <dt>Minecraft</dt>
                  <dd className="font-mono">{instance.versionId}</dd>
                </div>
                <div>
                  <dt>Mod Loader</dt>
                  <dd className="font-mono">
                    {instance.modLoader.type === "fabric"
                      ? `Fabric ${instance.modLoader.version}`
                      : "None"}
                  </dd>
                </div>
              </dl>
              <p className="mt-6 text-text-secondary">
                既存インスタンスのバージョン変更には未対応です。別のバージョンは新しいインスタンスとして作成してください。
              </p>
            </>
          )}
          {tab === "Mods" && (
            <>
              <div className="mb-4 flex flex-wrap items-center justify-between gap-4">
                <h3 className="text-section-title">Installed Mods</h3>
                <Button
                  disabled={
                    l.isRunning ||
                    Boolean(l.busy) ||
                    l.modOperationActive ||
                    instance.modLoader.type !== "fabric"
                  }
                  onClick={l.openMods}
                >
                  <Plus size={16} />
                  Add mods
                </Button>
              </div>
              {l.isRunning && (
                <p className="mb-4 text-warning-foreground">
                  実行中のファイル変更はできません。Minecraftを終了してください。
                </p>
              )}
              {instance.modLoader.type !== "fabric" && (
                <p className="mb-4 text-text-secondary">
                  Modrinthからの導入はFabricインスタンスで利用できます。
                </p>
              )}
              <InstalledMods launcher={l} />
            </>
          )}
          {tab === "Settings" && (
            <>
              <h3 className="text-section-title">General</h3>
              <div className="py-4">
                <label className="field">
                  表示名
                  <span className="field-hint">
                    表示名のみを変更します。ディレクトリ名は変わりません。
                  </span>
                  <input
                    maxLength={80}
                    value={l.settingsName}
                    disabled={l.busy === "rename"}
                    onChange={(event) => {
                      l.setSettingsName(event.target.value);
                      setSaved(false);
                    }}
                    onKeyDown={(event) => {
                      if (
                        event.key === "Enter" &&
                        !event.nativeEvent.isComposing &&
                        draft &&
                        l.settingsName.trim()
                      ) {
                        event.preventDefault();
                        void save();
                      }
                    }}
                  />
                </label>
                {draft && (
                  <div className="mt-3 flex gap-2">
                    <Button
                      tone="primary"
                      disabled={Boolean(l.busy) || !l.settingsName.trim()}
                      onClick={() => void save()}
                    >
                      {l.busy === "rename" ? "Saving…" : "Apply"}
                    </Button>
                    <Button
                      disabled={Boolean(l.busy)}
                      onClick={() => l.setSettingsName(instance.name)}
                    >
                      Cancel
                    </Button>
                  </div>
                )}
                {saved && <output className="mt-2 text-small">Saved</output>}
              </div>
              <h3 className="mt-8 text-section-title">Java</h3>
              <dl className="facts">
                <div>
                  <dt>Java path</dt>
                  <dd>
                    <span className="font-mono">{instance.javaPath}</span>
                    <CopyButton value={instance.javaPath} label="Java path" />
                  </dd>
                </div>
              </dl>
              <div className="mt-8 flex flex-wrap items-center justify-between gap-4 border-t border-border-subtle pt-6">
                <div>
                  <h3 className="text-navigation">インスタンスを削除</h3>
                  <p className="mt-1 text-small text-text-secondary">
                    このインスタンスのゲームデータを削除します。
                  </p>
                  {l.isRunning && <p className="mt-1 text-small">実行中は削除できません。</p>}
                </div>
                <Button
                  disabled={Boolean(l.busy) || l.isRunning || l.modOperationActive}
                  onClick={() => {
                    setDeleteName("");
                    setConfirm("delete");
                  }}
                >
                  削除…
                </Button>
              </div>
            </>
          )}
          {["Resource Packs", "Shader Packs", "Worlds", "Servers", "Screenshots"].includes(tab) && (
            <Empty title={`${tab}は未取得です`}>
              <p>
                現在のアプリはこのデータの読み込み・管理に対応していません。保存済みファイルの件数や状態は確認できていません。
              </p>
            </Empty>
          )}
        </div>
      </Dialog>
      {guard && (
        <Dialog
          title="未保存の変更があります"
          onClose={() => setGuard(false)}
          footer={
            <>
              <Button data-initial-focus onClick={() => setGuard(false)}>
                編集を続ける
              </Button>
              <Button
                disabled={Boolean(l.busy)}
                onClick={() => {
                  l.setSettingsName(instance.name);
                  setGuard(false);
                  pending.current?.();
                }}
              >
                破棄して移動
              </Button>
              <Button
                tone="primary"
                disabled={Boolean(l.busy) || !l.settingsName.trim()}
                onClick={() => {
                  void save().then((ok) => {
                    if (ok) {
                      setGuard(false);
                      pending.current?.();
                    }
                  });
                }}
              >
                保存して移動
              </Button>
            </>
          }
        >
          <div className="dialog-body">
            <p>変更した表示名を保存してから移動しますか？</p>
            <ErrorMessage>{l.error}</ErrorMessage>
          </div>
        </Dialog>
      )}
      {confirm && (
        <Dialog
          title={
            confirm === "stop"
              ? "Minecraftを強制終了しますか？"
              : confirm === "delete"
                ? "インスタンスを削除しますか？"
                : "管理対象ファイルを修復しますか？"
          }
          onClose={() => {
            if (!l.busy) setConfirm(null);
          }}
          footer={
            <>
              <Button
                data-initial-focus
                disabled={Boolean(l.busy)}
                onClick={() => setConfirm(null)}
              >
                Cancel
              </Button>
              <Button
                tone={confirm === "repair" ? "primary" : "danger"}
                disabled={
                  Boolean(l.busy) ||
                  (confirm === "delete" && (deleteName !== instance.name || l.isRunning))
                }
                onClick={() => {
                  if (confirm === "stop") {
                    setConfirm(null);
                    void l.stop();
                  } else if (confirm === "repair") {
                    setConfirm(null);
                    void l.repairSelected();
                  } else
                    void l.deleteSelected().then((ok) => {
                      if (ok) {
                        setConfirm(null);
                        onClose();
                      }
                    });
                }}
              >
                {l.busy === "delete"
                  ? "削除中…"
                  : confirm === "stop"
                    ? "Force Quit"
                    : confirm === "repair"
                      ? "修復する"
                      : "完全に削除"}
              </Button>
            </>
          }
        >
          <div className="dialog-body">
            <p className="mb-4 wrap-anywhere font-medium">{instance.name}</p>
            {confirm === "stop" ? (
              <p>保存されていないワールドの進行状況が失われる可能性があります。</p>
            ) : confirm === "repair" ? (
              <p>
                検証に失敗した管理対象ファイルを再取得します。Minecraftを終了してから実行してください。
              </p>
            ) : (
              <>
                <p>
                  この操作は取り消せません。対象ディレクトリ内のワールド・設定・ログ・画像も削除されます。
                </p>
                <p className="my-4 wrap-anywhere font-mono text-small">{instance.gameDirectory}</p>
                <p className="mb-4 text-small text-text-secondary">World件数は未取得です。</p>
                <label className="field">
                  確認のためインスタンス名を入力
                  <input
                    value={deleteName}
                    onChange={(event) => setDeleteName(event.target.value)}
                  />
                </label>
              </>
            )}
            <ErrorMessage>{l.error}</ErrorMessage>
          </div>
        </Dialog>
      )}
      {l.showMods && <ModCatalog launcher={l} />}
      {l.modRemovalTarget && (
        <Dialog
          title="Modを削除しますか？"
          onClose={() => {
            if (!l.modOperationActive) l.setModRemovalTarget(null);
          }}
          footer={
            <>
              <Button
                data-initial-focus
                disabled={l.modOperationActive}
                onClick={() => l.setModRemovalTarget(null)}
              >
                Cancel
              </Button>
              <Button
                tone="danger"
                disabled={l.modOperationActive || l.isRunning}
                onClick={() => void l.removeModrinthMod(l.modRemovalTarget!)}
              >
                {l.modRemovingProjectId ? "削除中…" : "削除する"}
              </Button>
            </>
          }
        >
          <div className="dialog-body">
            <p className="wrap-anywhere">{l.modRemovalTarget.title}</p>
            <p className="mt-4 text-text-secondary">
              不要になった必須依存も削除されます。他のModが必要とする依存は残ります。
            </p>
            <ErrorMessage>{l.modError}</ErrorMessage>
          </div>
        </Dialog>
      )}
    </>
  );
}

function InstalledMods({ launcher: l }: { launcher: Launcher }) {
  return (
    <>
      <ErrorMessage>{l.modError}</ErrorMessage>
      {l.modSuccess && <output className="my-4">{l.modSuccess}</output>}
      {l.installedModsLoading ? (
        <output>Modを読み込んでいます…</output>
      ) : !l.installedMods.length ? (
        <Empty title={l.modError ? "Mod一覧を取得できませんでした" : "管理対象のModはありません"}>
          <p>Modrinthで導入・管理しているModがここに表示されます。</p>
        </Empty>
      ) : (
        <div>
          {l.installedMods.map((mod) => (
            <div key={mod.projectId} className="mod-row">
              <div className="min-w-0 flex-1">
                <p className="wrap-anywhere text-navigation">{mod.title}</p>
                <p className="mt-1 wrap-anywhere font-mono text-small text-text-secondary">
                  {mod.versionNumber} · {mod.fileName}
                </p>
                <p className="mt-1 text-caption text-text-secondary">
                  {mod.direct ? "直接追加" : "必須依存"}
                </p>
              </div>
              {mod.direct && (
                <Button
                  disabled={l.isRunning || l.modOperationActive || Boolean(l.busy)}
                  onClick={() => l.setModRemovalTarget(mod)}
                >
                  削除…
                </Button>
              )}
            </div>
          ))}
        </div>
      )}
    </>
  );
}
function ModCatalog({ launcher: l }: { launcher: Launcher }) {
  return (
    <Dialog
      title="Add mods"
      large
      onClose={() => l.setShowMods(false)}
      footer={
        l.currentModInstallProgress ? (
          <Progress
            label={l.currentModInstallProgress.message}
            value={l.currentModInstallProgress.total > 0 ? l.modInstallPercent : undefined}
          />
        ) : undefined
      }
    >
      <div className="dialog-body flex-1">
        <form
          className="mb-6 flex flex-wrap gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void l.runModSearch(l.selected!.id, l.modQuery, 0);
          }}
        >
          <label className="min-w-0 flex-1">
            <span className="sr-only">Modrinthを検索</span>
            <input
              value={l.modQuery}
              onChange={(event) => l.setModQuery(event.target.value)}
              placeholder="Search Modrinth…"
            />
          </label>
          <Button type="submit" disabled={l.modSearchLoading}>
            <Search size={16} />
            検索
          </Button>
        </form>
        <p className="mb-4 text-small text-text-secondary">
          Minecraft {l.selected!.versionId} / Fabricに対応するModを検索します。
        </p>
        <ErrorMessage>{l.modError}</ErrorMessage>
        {l.modSuccess && <output className="my-4">{l.modSuccess}</output>}
        {l.modSearchLoading && <output>検索しています…</output>}
        {l.modSearch?.hits.map((mod) => (
          <div className="mod-row" key={mod.projectId}>
            <div className="min-w-0 flex-1">
              <h3 className="text-navigation">{mod.title}</h3>
              <p className="mt-1 wrap-anywhere text-text-secondary">{mod.description}</p>
              <p className="mt-2 text-caption text-text-secondary">
                {mod.author} · {mod.downloads.toLocaleString("ja-JP")} downloads
              </p>
            </div>
            <Button
              disabled={
                l.modOperationActive || l.isRunning || l.installedProjectIds.has(mod.projectId)
              }
              onClick={() => void l.installModrinthMod(mod.projectId)}
            >
              {l.modInstallingProjectId === mod.projectId
                ? "導入中…"
                : l.installedProjectIds.has(mod.projectId)
                  ? "導入済み"
                  : "導入"}
            </Button>
          </div>
        ))}
        {!l.modSearchLoading && l.modSearch?.hits.length === 0 && (
          <Empty title="一致するModがありません">検索語を変更してください。</Empty>
        )}
        {l.modSearch && (
          <div className="mt-6 flex flex-wrap items-center gap-4">
            <Button
              disabled={l.modSearchLoading || l.modSearch.offset === 0}
              onClick={() =>
                void l.runModSearch(
                  l.selected!.id,
                  l.modQuery,
                  Math.max(0, l.modSearch!.offset - l.modSearch!.limit),
                )
              }
            >
              前へ
            </Button>
            <span className="text-small">{l.modSearch.totalHits}件</span>
            <Button
              disabled={
                l.modSearchLoading ||
                l.modSearch.offset + l.modSearch.limit >= l.modSearch.totalHits
              }
              onClick={() =>
                void l.runModSearch(
                  l.selected!.id,
                  l.modQuery,
                  l.modSearch!.offset + l.modSearch!.limit,
                )
              }
            >
              次へ
            </Button>
          </div>
        )}
      </div>
    </Dialog>
  );
}
function LogPanel({ launcher: l }: { launcher: Launcher }) {
  const [query, setQuery] = useState("");
  const [level, setLevel] = useState("all");
  const [follow, setFollow] = useState(true);
  const [wrap, setWrap] = useState(false);
  const output = useRef<HTMLDivElement>(null);
  const lines = l.visibleLogs.filter(
    (line) =>
      line.line.toLowerCase().includes(query.toLowerCase()) &&
      (level === "all" ||
        line.line.toUpperCase().includes(level) ||
        (level === "ERROR" && line.stream === "stderr")),
  );
  const text = lines.map((line) => `[${line.stream}] ${line.line}`).join("\n");
  useEffect(() => {
    if (follow && output.current && !window.getSelection()?.toString())
      output.current.scrollTop = output.current.scrollHeight;
  }, [follow, l.visibleLogs.length]);
  return (
    <>
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <label className="min-w-0 flex-1">
          <span className="sr-only">ログを検索</span>
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search logs…"
          />
        </label>
        <label>
          <span className="sr-only">ログレベル</span>
          <select value={level} onChange={(event) => setLevel(event.target.value)}>
            <option value="all">All levels</option>
            <option>DEBUG</option>
            <option>WARN</option>
            <option>ERROR</option>
          </select>
        </label>
        <CopyButton value={text} label="表示中のログ" />
        <Button
          onClick={() => {
            const url = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
            const anchor = document.createElement("a");
            anchor.href = url;
            anchor.download = "minecraft.log";
            anchor.click();
            setTimeout(() => URL.revokeObjectURL(url), 1000);
          }}
        >
          <Download size={16} />
          Export
        </Button>
      </div>
      <div className="mb-3 flex flex-wrap items-center gap-4">
        <label className="flex min-h-8 items-center gap-2">
          <input
            type="checkbox"
            checked={wrap}
            onChange={(event) => setWrap(event.target.checked)}
          />
          Wrap
        </label>
        <Button
          aria-pressed={follow}
          onClick={() => {
            setFollow(!follow);
            if (!follow && output.current) output.current.scrollTop = output.current.scrollHeight;
          }}
        >
          {follow ? "Follow: on" : "最新へ移動"}
        </Button>
      </div>
      <div
        ref={output}
        // The scrollable log must be keyboard focusable for local horizontal scrolling.
        // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
        tabIndex={0}
        role="log"
        aria-live="off"
        aria-label="Minecraftログ"
        className={`log-lines ${wrap ? "log-wrap" : ""}`}
        onScroll={(event) => {
          const el = event.currentTarget;
          if (el.scrollHeight - el.scrollTop - el.clientHeight > 32) setFollow(false);
        }}
      >
        {!lines.length ? (
          <p className="text-log-debug">
            {l.visibleLogs.length
              ? "条件に一致するログはありません。"
              : "起動すると、このインスタンスのログが表示されます。"}
          </p>
        ) : (
          lines.map((line) => (
            <div
              key={line.id}
              className={
                line.stream === "stderr" || /ERROR/.test(line.line)
                  ? "text-log-error"
                  : /WARN/.test(line.line)
                    ? "text-log-warning"
                    : /DEBUG/.test(line.line)
                      ? "text-log-debug"
                      : ""
              }
            >
              <code>
                [{line.stream}] {line.line}
              </code>
            </div>
          ))
        )}
      </div>
    </>
  );
}

export function CreateDialog({
  launcher: l,
  onCreated,
}: {
  launcher: Launcher;
  onCreated: (item: MinecraftInstance) => void;
}) {
  const [discard, setDiscard] = useState(false);
  const [releaseOnly, setReleaseOnly] = useState(
    () =>
      l.versionCatalog?.versions.find((version) => version.id === l.selectedVersionId)
        ?.versionType !== "snapshot",
  );
  const baseline = useRef(
    JSON.stringify([l.instanceName, l.selectedVersionId, l.modLoaderType, l.gameMode]),
  );
  const valid =
    l.instanceName.trim() &&
    l.selectedVersionId &&
    (l.modLoaderType === "vanilla" || l.selectedFabricLoader) &&
    !l.versionsLoading &&
    !l.fabricLoadersLoading;
  // Generate the stable instance id before calling the controller; creation uses the current form state.
  const create = async () => {
    const item = await l.install();
    if (item) onCreated(item);
  };

  const close = () => {
    if (l.busy === "install") l.setShowCreator(false);
    else if (
      baseline.current !==
      JSON.stringify([l.instanceName, l.selectedVersionId, l.modLoaderType, l.gameMode])
    )
      setDiscard(true);
    else l.setShowCreator(false);
  };
  return (
    <>
      <Dialog
        title="Create instance"
        onClose={close}
        footer={
          <>
            <Button onClick={close}>Cancel</Button>
            <Button
              tone="primary"
              disabled={!valid || Boolean(l.busy)}
              onClick={() => void create()}
            >
              {l.busy === "install" ? "Creating…" : "Create"}
            </Button>
          </>
        }
      >
        <div className="dialog-body">
          <form
            className="flex flex-col gap-4"
            onSubmit={(event) => {
              event.preventDefault();
              if (valid && !l.busy) void create();
            }}
          >
            <label className="field">
              Name
              <input
                maxLength={80}
                value={l.instanceName}
                disabled={Boolean(l.busy)}
                onChange={(event) => l.setInstanceName(event.target.value)}
                required
              />
            </label>
            <label className="field">
              Minecraft versionを検索
              <input
                value={l.versionQuery}
                onChange={(event) => l.setVersionQuery(event.target.value)}
                placeholder="例: 1.21"
              />
            </label>
            <label className="flex min-h-8 items-center gap-2">
              <input
                type="checkbox"
                checked={releaseOnly}
                onChange={(event) => {
                  setReleaseOnly(event.target.checked);
                  if (
                    event.target.checked &&
                    l.versionCatalog?.versions.find((version) => version.id === l.selectedVersionId)
                      ?.versionType !== "release"
                  )
                    l.setSelectedVersionId(l.versionCatalog?.latest.release ?? "");
                }}
              />
              Releaseのみ
            </label>
            <label className="field">
              Minecraft version
              <select
                required
                value={l.selectedVersionId}
                disabled={l.versionsLoading || Boolean(l.busy)}
                onChange={(event) => l.setSelectedVersionId(event.target.value)}
              >
                {!l.versionCatalog && <option value="">バージョンを取得できません</option>}
                {l.versionGroups
                  .filter((group) => !releaseOnly || group.type === "release")
                  .map((group) => (
                    <optgroup key={group.type} label={group.label}>
                      {group.versions.map((version) => (
                        <option key={version.id} value={version.id}>
                          {version.id}
                          {version.id === l.versionCatalog?.latest.release
                            ? " — Latest release"
                            : ""}
                        </option>
                      ))}
                    </optgroup>
                  ))}
              </select>
            </label>
            {l.versionsLoading && <output>バージョンを読み込んでいます…</output>}
            <ErrorMessage>{l.versionsError}</ErrorMessage>
            {l.versionsError && (
              <Button onClick={() => void l.refreshVersions().catch(() => {})}>再試行</Button>
            )}
            <label className="field">
              Mod Loader
              <select
                value={l.modLoaderType}
                disabled={Boolean(l.busy)}
                onChange={(event) => l.setModLoaderType(event.target.value as "vanilla" | "fabric")}
              >
                <option value="vanilla">None</option>
                <option value="fabric">Fabric</option>
              </select>
            </label>
            {l.modLoaderType === "fabric" && (
              <>
                <label className="field">
                  Fabric Loader
                  <select
                    value={l.selectedFabricLoader}
                    disabled={l.fabricLoadersLoading || Boolean(l.busy)}
                    onChange={(event) => l.setSelectedFabricLoader(event.target.value)}
                  >
                    {!l.fabricLoaders.length && (
                      <option value="">
                        {l.fabricLoadersLoading ? "取得中…" : "対応するLoaderがありません"}
                      </option>
                    )}
                    {l.fabricLoaders.map((loader) => (
                      <option key={loader.version}>{loader.version}</option>
                    ))}
                  </select>
                </label>
                <ErrorMessage>{l.fabricLoadersError}</ErrorMessage>
                {l.fabricLoadersError && (
                  <Button onClick={() => l.setFabricReloadKey((key) => key + 1)}>
                    Loaderを再取得
                  </Button>
                )}
              </>
            )}
            <label className="field">
              ゲームモード
              <select
                value={l.gameMode}
                disabled={Boolean(l.busy)}
                onChange={(event) => l.setGameMode(event.target.value as "offline" | "demo")}
              >
                <option value="offline">通常</option>
                <option value="demo">デモ</option>
              </select>
            </label>
            <p className="text-small text-text-secondary">
              起動可否はアカウントの権利と起動基盤で確認されます。
            </p>
            <ErrorMessage>{l.creatorError}</ErrorMessage>
            {l.progress && (
              <Progress
                label={l.progress.message}
                value={l.progress.total > 0 ? l.progressPercent : undefined}
              />
            )}
          </form>
        </div>
      </Dialog>
      {discard && (
        <Dialog
          title="作成内容を破棄しますか？"
          onClose={() => setDiscard(false)}
          footer={
            <>
              <Button data-initial-focus onClick={() => setDiscard(false)}>
                編集を続ける
              </Button>
              <Button
                onClick={() => {
                  setDiscard(false);
                  l.setInstanceName("Minecraft");
                  l.setModLoaderType("vanilla");
                  l.setSelectedVersionId(l.versionCatalog?.latest.release ?? "");
                  l.setVersionQuery("");
                  l.setGameMode("offline");
                  l.setShowCreator(false);
                }}
              >
                破棄して閉じる
              </Button>
            </>
          }
        >
          <div className="dialog-body">入力した内容はまだ作成されていません。</div>
        </Dialog>
      )}
    </>
  );
}
export function AuthDialog({ launcher: l }: { launcher: Launcher }) {
  return (
    <Dialog title="Microsoft account" onClose={() => l.setShowAuth(false)}>
      <div className="dialog-body">
        <p className="mb-4 text-text-secondary">
          Microsoftアカウントを使ってMinecraftのプロフィールを確認します。
        </p>
        <ErrorMessage>{l.authError}</ErrorMessage>
        {!l.authStatus.configured ? (
          <p>このビルドにはMicrosoft認証が構成されていません。</p>
        ) : l.authStatus.authorized ? (
          <>
            <p className="text-navigation">{l.minecraftProfile?.name ?? "Microsoft認証済み"}</p>
            <p className="mt-2 text-small">
              {l.minecraftProfileLoading
                ? "Minecraftプロフィールを確認中…"
                : l.minecraftProfile
                  ? "Minecraft: Java Edition"
                  : "Minecraftプロフィールを確認できていません。"}
            </p>
            <div className="mt-6 flex flex-wrap gap-2">
              <Button
                disabled={l.minecraftProfileLoading}
                onClick={() => void l.refreshMinecraftProfile()}
              >
                プロフィールを再確認
              </Button>
              <Button disabled={Boolean(l.authBusy)} onClick={() => void l.signOutMicrosoft()}>
                Sign out
              </Button>
            </div>
          </>
        ) : (
          <>
            <Button
              tone="primary"
              disabled={Boolean(l.authBusy) || Boolean(l.authChallenge)}
              onClick={() => void l.beginMicrosoftSignIn()}
            >
              {l.authBusy ? "準備しています…" : "Microsoftでサインイン"}
            </Button>
            {l.authChallenge && (
              <div className="mt-6">
                <p>ブラウザーで次のコードを入力してください。</p>
                <div className="my-4 flex flex-wrap items-center gap-2">
                  <code className="font-mono text-section-title">{l.authChallenge.userCode}</code>
                  <CopyButton label="認証コード" value={l.authChallenge.userCode} />
                </div>
                <Button
                  onClick={() => void l.openMicrosoftVerification(l.authChallenge!.verificationUri)}
                >
                  <ExternalLink size={16} />
                  ブラウザーを開く
                </Button>
                <p className="mt-2 wrap-anywhere font-mono text-small">
                  {l.authChallenge.verificationUri}
                </p>
                <output className="mt-4 text-small">認証の完了を待っています…</output>
              </div>
            )}
          </>
        )}
      </div>
    </Dialog>
  );
}
