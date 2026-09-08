import { useEffect, useRef, useState } from "react";
import {
  Box,
  Download,
  Home,
  Images,
  Menu,
  MoreHorizontal,
  Newspaper,
  Play,
  Plus,
  Search,
  Settings,
  SlidersHorizontal,
} from "lucide-react";
import {
  hasTauriRuntime,
  instanceVersionLabel,
  useLauncher,
  type MinecraftInstance,
} from "./hooks/useLauncher";
import { Button, Dialog, Empty, ErrorMessage, Progress, Tabs } from "./components/ui";
import { AuthDialog, CreateDialog, InstanceDialog } from "./components/launcher-dialogs";

const pages = ["Home", "Instances", "News", "Gallery", "Settings"] as const;
type Page = (typeof pages)[number];
const icons = { Home, Instances: Box, News: Newspaper, Gallery: Images, Settings };
function readHistory(): Record<string, number> {
  try {
    const value: unknown = JSON.parse(localStorage.getItem("mona:last-played") ?? "{}");
    return value && typeof value === "object" && !Array.isArray(value)
      ? Object.fromEntries(
          Object.entries(value).filter(
            ([, time]) => typeof time === "number" && Number.isFinite(time),
          ),
        )
      : {};
  } catch {
    return {};
  }
}
function readTheme() {
  try {
    const value = localStorage.getItem("mona:theme");
    return value === "light" || value === "dark" ? value : "system";
  } catch {
    return "system";
  }
}
export default function App() {
  const launcher = useLauncher();
  const [page, setPage] = useState<Page>("Home");
  const [drawer, setDrawer] = useState(false);
  const [instanceOpen, setInstanceOpen] = useState(false);
  const [initialTab, setInitialTab] = useState("Overview");
  const [query, setQuery] = useState("");
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [version, setVersion] = useState("");
  const [loader, setLoader] = useState("");
  const [sort, setSort] = useState("recent");
  const [theme, setTheme] = useState(readTheme);
  const [preferenceError, setPreferenceError] = useState("");
  const [history, setHistory] = useState(readHistory);
  const previousRunning = useRef(new Set<string>());
  const [settingsTab, setSettingsTab] = useState("General");
  const tabMemory = useRef<Record<string, string>>({});
  const scrollMemory = useRef<Record<string, number>>({});
  const { selected, instances, runningIds, busy, progress, launchProgress } = launcher;
  const native = hasTauriRuntime();
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
  useEffect(() => {
    const newlyRunning = [...runningIds].filter((id) => !previousRunning.current.has(id));
    previousRunning.current = new Set(runningIds);
    if (!newlyRunning.length) return;
    setHistory((current) => {
      const next = {
        ...current,
        ...Object.fromEntries(newlyRunning.map((id) => [id, Date.now()])),
      };
      try {
        localStorage.setItem("mona:last-played", JSON.stringify(next));
      } catch {
        /* In-memory history remains available. */
      }
      return next;
    });
  }, [runningIds]);
  const navigate = (next: Page) => {
    setPage(next);
    setDrawer(false);
  };
  useEffect(() => {
    if (!native) return;
    const keydown = (event: KeyboardEvent) => {
      if (
        event.isComposing ||
        !(event.metaKey || event.ctrlKey) ||
        document.querySelector("dialog[open]")
      )
        return;
      const next = event.key === "," ? "Settings" : pages[Number(event.key) - 1];
      if (next && (event.key === "," || /^[1-4]$/.test(event.key))) {
        event.preventDefault();
        setPage(next);
      }
    };
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  }, [native]);
  const openInstance = (instance: MinecraftInstance, tab?: string) => {
    if ((busy || launcher.modOperationActive) && selected?.id !== instance.id) return;
    launcher.setSelectedId(instance.id);
    launcher.setSettingsName(instance.name);
    setInitialTab(tab ?? tabMemory.current[instance.id] ?? "Overview");
    setInstanceOpen(true);
    setDrawer(false);
  };
  const recent = instances
    .filter((item) => history[item.id])
    .sort((a, b) => history[b.id] - history[a.id]);
  const playing = instances
    .filter((item) => runningIds.has(item.id))
    .sort((a, b) => (history[b.id] ?? 0) - (history[a.id] ?? 0));
  const filtered = instances
    .filter(
      (item) =>
        `${item.name} ${item.versionId}`.toLowerCase().includes(query.toLowerCase()) &&
        (!version || item.versionId === version) &&
        (!loader || item.modLoader.type === loader),
    )
    .sort(
      (a, b) =>
        (sort === "recent" ? (history[b.id] ?? 0) - (history[a.id] ?? 0) : 0) ||
        a.name.localeCompare(b.name, "ja") ||
        a.id.localeCompare(b.id),
    );
  const operationLabel =
    launchProgress?.message ??
    progress?.message ??
    launcher.modInstallProgress?.message ??
    (busy === "stop"
      ? "Minecraftを終了しています…"
      : busy === "diagnose"
        ? "ファイルを検証しています…"
        : null);
  const Sidebar = (
    <aside className="sidebar" aria-label="アプリケーションナビゲーション">
      <nav aria-label="メインメニュー" className="min-h-0 flex-1 overflow-y-auto">
        <div className="flex flex-col gap-1">
          {pages.slice(0, 4).map((item) => {
            const Icon = icons[item];
            return (
              <button
                key={item}
                className="nav-item"
                aria-current={page === item ? "page" : undefined}
                onClick={() => navigate(item)}
              >
                <Icon size={18} strokeWidth={2} />
                <span>{item}</span>
              </button>
            );
          })}
        </div>
      </nav>
      <div className="mt-6 shrink-0">
        {operationLabel && (
          <div className="mb-6 px-3">
            <div className="mb-2 flex items-center gap-2 text-small">
              <Download size={16} />
              Activity
            </div>
            <output className="wrap-anywhere text-small text-text-secondary">
              {operationLabel}
            </output>
            {(selected || busy === "install") && (
              <Button
                tone="ghost"
                onClick={() => {
                  if (busy === "install") launcher.setShowCreator(true);
                  else if (selected) openInstance(selected);
                }}
              >
                処理の詳細
              </Button>
            )}
          </div>
        )}
        {(playing[0] || recent[0]) && (
          <button
            className="nav-item mb-6 flex-wrap text-left"
            onClick={() => openInstance(playing[0] ?? recent[0])}
            disabled={Boolean(
              (busy || launcher.modOperationActive) &&
              selected?.id !== (playing[0] ?? recent[0]).id,
            )}
          >
            <span className="w-full text-caption text-text-secondary">
              {playing.length ? "Now Playing" : "Last Played"}
            </span>
            <span className="min-w-0 flex-1 truncate">{(playing[0] ?? recent[0]).name}</span>
            {playing.length > 0 && (
              <span className="text-caption text-success-foreground">
                Running{playing.length > 1 ? ` · ほか${playing.length - 1}件` : ""}
              </span>
            )}
          </button>
        )}
        <div className="border-t border-border-subtle pt-3">
          <button
            className="nav-item"
            aria-current={page === "Settings" ? "page" : undefined}
            onClick={() => navigate("Settings")}
          >
            <Settings size={18} />
            Settings
          </button>
        </div>
      </div>
    </aside>
  );
  const createButton = (
    <Button tone="primary" disabled={!native || Boolean(busy)} onClick={launcher.openCreator}>
      <Plus size={16} />
      Add instance
    </Button>
  );
  const rows = (items: MinecraftInstance[]) => (
    <div aria-label="Minecraftインスタンス">
      {items.map((item) => (
        <div className="instance-row" key={item.id}>
          <span className="instance-avatar" aria-hidden="true">
            <Box size={20} />
          </span>
          <div className="min-w-0 flex-1">
            <button className="instance-name" title={item.name} onClick={() => openInstance(item)}>
              {item.name}
            </button>
            <p className="mt-1 truncate font-mono text-caption text-text-secondary">
              {instanceVersionLabel(item)}
            </p>
          </div>
          {runningIds.has(item.id) && (
            <span className="text-small text-success-foreground">Running</span>
          )}
          <div className="ml-auto flex shrink-0 items-center gap-2">
            <Button
              tone={runningIds.has(item.id) ? "secondary" : "primary"}
              disabled={Boolean(busy) || launcher.modOperationActive || !item.sandboxed}
              onClick={() => {
                if (runningIds.has(item.id)) openInstance(item);
                else {
                  launcher.setSettingsName(item.name);
                  void launcher.launch(item);
                }
              }}
            >
              {runningIds.has(item.id) ? (
                "Open instance"
              ) : (
                <>
                  <Play size={14} />
                  Play
                </>
              )}
            </Button>
            <details className="relative">
              <summary
                className="button button-ghost icon-button list-none"
                aria-label={`${item.name}の操作`}
              >
                <MoreHorizontal size={18} />
              </summary>
              <div className="absolute right-0 z-10 mt-1 min-w-(--mona-layout-sidebar-width) rounded-menu border border-border-subtle bg-background-floating p-1 shadow-floating">
                <Button
                  tone="ghost"
                  className="w-full justify-start"
                  onClick={(event) => {
                    event.currentTarget.closest("details")?.removeAttribute("open");
                    openInstance(item, "Settings");
                  }}
                >
                  インスタンス設定
                </Button>
              </div>
            </details>
          </div>
        </div>
      ))}
    </div>
  );
  return (
    <div className="design-surface flex h-dvh overflow-hidden">
      <div className="hidden shell:block">{Sidebar}</div>
      <main className="min-h-0 min-w-0 flex-1 overflow-y-auto p-4 shell:p-8">
        <div className="mb-4 shell:hidden">
          <Button tone="ghost" aria-label="ナビゲーションを開く" onClick={() => setDrawer(true)}>
            <Menu size={18} />
            Menu
          </Button>
        </div>
        <div
          className={
            page === "Home"
              ? "max-w-home"
              : page === "News"
                ? "max-w-news"
                : page === "Settings"
                  ? "max-w-settings"
                  : ""
          }
        >
          <header className="mb-6 flex flex-wrap items-center justify-between gap-4">
            <h1 className="text-page-title text-text-heading">{page}</h1>
            {(page === "Instances" || (page === "Home" && instances.length > 0)) && createButton}
          </header>
          {!native && (
            <p className="mb-6 text-small text-text-secondary">
              ブラウザープレビューです。インスタンス操作はデスクトップアプリで利用できます。
            </p>
          )}
          {!instanceOpen && <ErrorMessage>{launcher.error}</ErrorMessage>}
          {page === "Home" && (
            <>
              {playing.length > 0 && (
                <section className="mb-8">
                  <h2 className="mb-4 text-section-title">Now Playing</h2>
                  {rows(playing)}
                </section>
              )}
              {launcher.instancesLoading ? (
                <output>インスタンスを読み込んでいます…</output>
              ) : instances.length === 0 ? (
                <Empty title="最初のインスタンスを作成" action={createButton}>
                  <p>MinecraftのバージョンとMod Loaderを選んで、独立した環境を作成できます。</p>
                </Empty>
              ) : (
                <section className="mb-8">
                  <div className="mb-4 flex items-center justify-between gap-4">
                    <h2 className="text-section-title">Recent Instances</h2>
                    <Button tone="ghost" onClick={() => navigate("Instances")}>
                      View all
                    </Button>
                  </div>
                  {recent.length ? (
                    rows(recent.slice(0, 3))
                  ) : (
                    <p className="text-text-secondary">
                      最近使用した履歴はありません。Instancesから起動できます。
                    </p>
                  )}
                </section>
              )}
              {!launcher.authStatus.authorized && (
                <section className="mb-8 flex flex-wrap items-center justify-between gap-4 border-b border-border-subtle py-4">
                  <div>
                    <h2 className="text-navigation">Microsoftアカウント</h2>
                    <p className="mt-1 text-small text-text-secondary">
                      MinecraftのアカウントをSettingsから管理できます。
                    </p>
                  </div>
                  <Button
                    onClick={() => {
                      navigate("Settings");
                      setSettingsTab("General");
                    }}
                  >
                    Sign in
                  </Button>
                </section>
              )}
              <section className="mb-8">
                <div className="mb-4 flex items-center justify-between">
                  <h2 className="text-section-title">Recent Screenshots</h2>
                  <Button tone="ghost" onClick={() => navigate("Gallery")}>
                    View all
                  </Button>
                </div>
                <p className="text-text-secondary">
                  スクリーンショットは未取得です。現在のアプリは画像の読み込みに対応していません。
                </p>
              </section>
              <section>
                <div className="mb-4 flex items-center justify-between">
                  <h2 className="text-section-title">News</h2>
                  <Button tone="ghost" onClick={() => navigate("News")}>
                    View all
                  </Button>
                </div>
                <p className="text-text-secondary">
                  ニュースは未取得です。配信元との連携はまだありません。
                </p>
              </section>
            </>
          )}
          {page === "Instances" && (
            <>
              <div className="mb-4 flex flex-wrap items-center gap-2">
                <label className="relative min-w-0 flex-1">
                  <Search
                    size={16}
                    className="pointer-events-none absolute top-3 left-3 text-text-secondary"
                  />
                  <span className="sr-only">インスタンスを検索</span>
                  <input
                    className="pl-10"
                    placeholder="Search instances…"
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                  />
                </label>
                <Button aria-expanded={filtersOpen} onClick={() => setFiltersOpen(!filtersOpen)}>
                  <SlidersHorizontal size={16} />
                  Filter
                  {(version || loader) &&
                    ` (${Number(Boolean(version)) + Number(Boolean(loader))})`}
                </Button>
                <label>
                  <span className="sr-only">並び順</span>
                  <select value={sort} onChange={(event) => setSort(event.target.value)}>
                    <option value="recent">Last Played</option>
                    <option value="name">Name</option>
                  </select>
                </label>
              </div>
              {filtersOpen && (
                <div className="mb-4 flex flex-wrap items-end gap-4">
                  <label className="field">
                    Minecraft version
                    <select value={version} onChange={(event) => setVersion(event.target.value)}>
                      <option value="">All versions</option>
                      {[...new Set(instances.map((item) => item.versionId))].map((id) => (
                        <option key={id}>{id}</option>
                      ))}
                    </select>
                  </label>
                  <label className="field">
                    Mod Loader
                    <select value={loader} onChange={(event) => setLoader(event.target.value)}>
                      <option value="">All loaders</option>
                      {[...new Set(instances.map((item) => item.modLoader.type))].map((id) => (
                        <option key={id} value={id}>
                          {id === "fabric" ? "Fabric" : "Vanilla"}
                        </option>
                      ))}
                    </select>
                  </label>
                  <Button
                    tone="ghost"
                    onClick={() => {
                      setVersion("");
                      setLoader("");
                    }}
                  >
                    Clear filters
                  </Button>
                </div>
              )}
              {launcher.instancesLoading ? (
                <output className="py-16">インスタンスを読み込んでいます…</output>
              ) : !instances.length ? (
                <Empty
                  title={
                    launcher.error
                      ? "インスタンスを取得できませんでした"
                      : "インスタンスはまだありません"
                  }
                  action={
                    launcher.error ? (
                      <Button
                        onClick={() =>
                          void launcher
                            .refreshInstances()
                            .catch((cause) => launcher.setError(String(cause)))
                        }
                      >
                        再試行
                      </Button>
                    ) : (
                      createButton
                    )
                  }
                >
                  独立したMinecraft環境を追加してください。
                </Empty>
              ) : !filtered.length ? (
                <Empty
                  title="一致するインスタンスがありません"
                  action={
                    <Button
                      onClick={() => {
                        setQuery("");
                        setVersion("");
                        setLoader("");
                      }}
                    >
                      検索条件をクリア
                    </Button>
                  }
                >
                  名前やフィルターを変更してください。
                </Empty>
              ) : (
                <>
                  <p className="mb-2 text-small text-text-secondary">{filtered.length} instances</p>
                  {rows(filtered)}
                </>
              )}
            </>
          )}
          {(page === "News" || page === "Gallery") && (
            <Empty
              title={page === "News" ? "ニュースは未取得です" : "スクリーンショットは未取得です"}
            >
              <p>
                {page === "News"
                  ? "現在のバージョンにはニュース配信を取得する機能がありません。"
                  : "現在のバージョンには保存済みスクリーンショットを読み込む機能がありません。ゲームの画像は削除されていません。"}
              </p>
            </Empty>
          )}
          {page === "Settings" && (
            <>
              <Tabs
                id="settings"
                tabs={["General", "Minecraft", "Java", "Advanced"]}
                active={settingsTab}
                onChange={setSettingsTab}
              />
              <section
                role="tabpanel"
                id="settings-panel"
                aria-labelledby={`settings-tab-${["General", "Minecraft", "Java", "Advanced"].indexOf(settingsTab)}`}
                className="pt-6"
              >
                {settingsTab === "General" ? (
                  <>
                    <h2 className="text-section-title">Appearance</h2>
                    <div className="setting-row">
                      <div>
                        <label htmlFor="theme" className="text-navigation">
                          Theme
                        </label>
                        <p className="mt-1 text-small text-text-secondary">
                          アプリの外観。SystemはOSの設定に従います。
                        </p>
                      </div>
                      <select
                        id="theme"
                        className="max-w-[12.5rem]"
                        value={theme}
                        onChange={(event) => {
                          const next = event.target.value;
                          try {
                            localStorage.setItem("mona:theme", next);
                            setTheme(next);
                            setPreferenceError("");
                          } catch {
                            setPreferenceError(
                              "外観を保存できませんでした。もう一度お試しください。",
                            );
                          }
                        }}
                      >
                        <option value="system">System</option>
                        <option value="light">Light</option>
                        <option value="dark">Dark</option>
                      </select>
                    </div>
                    <ErrorMessage>{preferenceError}</ErrorMessage>
                    <h2 className="mt-8 text-section-title">Language</h2>
                    <div className="setting-row">
                      <div>
                        <p className="text-navigation">表示言語</p>
                        <p className="mt-1 text-small text-text-secondary">
                          現在は日本語の説明と英語のナビゲーションに対応しています。
                        </p>
                      </div>
                      <span>日本語</span>
                    </div>
                    <h2 className="mt-8 text-section-title">Accounts</h2>
                    <div className="setting-row">
                      <div>
                        <p className="text-navigation">
                          {launcher.minecraftProfile?.name ?? "Microsoft account"}
                        </p>
                        <p className="mt-1 text-small text-text-secondary">
                          {launcher.minecraftProfileLoading
                            ? "プロフィールを確認しています…"
                            : launcher.authStatus.authorized
                              ? "Microsoft認証済み"
                              : launcher.authStatus.configured
                                ? "未設定 · サインインできます"
                                : "Microsoft認証の構成が必要です"}
                        </p>
                      </div>
                      <Button disabled={!native} onClick={launcher.openAuth}>
                        {launcher.authStatus.authorized ? "アカウントを管理" : "Sign in"}
                      </Button>
                    </div>
                    <ErrorMessage>{launcher.authError}</ErrorMessage>
                  </>
                ) : (
                  <Empty title={`${settingsTab} settings`}>
                    <p>
                      この分類のグローバル設定はまだ変更できません。インスタンス固有の値は、各インスタンスの詳細で確認できます。
                    </p>
                  </Empty>
                )}
              </section>
            </>
          )}
        </div>
      </main>
      {drawer && (
        <Dialog title="Navigation" onClose={() => setDrawer(false)} className="drawer">
          {Sidebar}
        </Dialog>
      )}
      {instanceOpen && selected && (
        <InstanceDialog
          key={selected.id}
          launcher={launcher}
          initialTab={initialTab}
          rememberTab={(tab) => {
            tabMemory.current[selected.id] = tab;
          }}
          scrollMemory={scrollMemory.current}
          onClose={() => setInstanceOpen(false)}
        />
      )}
      {launcher.showCreator && (
        <CreateDialog
          launcher={launcher}
          onCreated={(item) => {
            setInitialTab("Overview");
            launcher.setSettingsName(item.name);
            setInstanceOpen(true);
          }}
        />
      )}
      {launcher.showAuth && <AuthDialog launcher={launcher} />}
      {!instanceOpen && operationLabel && progress && (
        <div className="sr-only">
          <Progress
            label={operationLabel}
            value={progress.total > 0 ? launcher.progressPercent : undefined}
          />
        </div>
      )}
    </div>
  );
}
