import { Plus } from "lucide-react";
import { AppShell } from "./app/AppShell";
import { useNavigation } from "./app/navigation";
import { useLauncher } from "./app/useLauncher";
import { Button } from "./components/Button";
import { Empty } from "./components/Empty";
import { ErrorMessage } from "./components/ErrorMessage";
import { AuthDialog } from "./features/auth/AuthDialog";
import { HomePage } from "./features/home/HomePage";
import { NewsPage } from "./features/news/NewsPage";
import { useNews } from "./features/news/useNews";
import { CreateDialog } from "./features/instances/create/CreateDialog";
import { InstanceDialog } from "./features/instances/detail/InstanceDialog";
import { InstanceBrowser } from "./features/instances/InstanceBrowser";
import { useInstanceFilters } from "./features/instances/useInstanceFilters";
import { useInstanceNavigation } from "./features/instances/useInstanceNavigation";
import { usePlayHistory } from "./features/instances/usePlayHistory";
import { SettingsPage } from "./features/settings/SettingsPage";
import { hasTauriRuntime } from "./lib/tauri";

export default function App() {
  const launcher = useLauncher();
  const news = useNews();
  const { page, navigate, settingsTab, setSettingsTab, openAccountSettings } = useNavigation();
  const history = usePlayHistory(launcher.instances, launcher.runningIds);
  const filters = useInstanceFilters();
  const dialog = useInstanceNavigation(launcher);
  const native = hasTauriRuntime();
  const createButton = (
    <Button
      tone="primary"
      disabled={!native || Boolean(launcher.busy)}
      onClick={launcher.openCreator}
    >
      <Plus size={16} />
      Add instance
    </Button>
  );
  return (
    <>
      <AppShell
        page={page}
        navigate={navigate}
        launcher={launcher}
        playing={history.playing}
        recent={history.recent}
        openInstance={dialog.openInstance}
      >
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
            {(page === "Instances" || (page === "Home" && launcher.instances.length > 0)) &&
              createButton}
          </header>
          {!native && (
            <p className="mb-6 text-small text-text-secondary">
              ブラウザープレビューです。インスタンス操作はデスクトップアプリで利用できます。
            </p>
          )}
          {!dialog.instanceOpen && <ErrorMessage>{launcher.error}</ErrorMessage>}
          {page === "Home" && (
            <HomePage
              news={news}
              onSignIn={openAccountSettings}
              launcher={launcher}
              {...history}
              openInstance={dialog.openInstance}
              createButton={createButton}
              navigate={navigate}
            />
          )}
          {page === "Instances" && (
            <InstanceBrowser
              launcher={launcher}
              history={history.history}
              filters={filters}
              openInstance={dialog.openInstance}
              createButton={createButton}
            />
          )}
          {page === "Settings" && (
            <SettingsPage
              launcher={launcher}
              settingsTab={settingsTab}
              setSettingsTab={setSettingsTab}
            />
          )}
          {page === "News" && <NewsPage news={news} />}
          {page === "Gallery" && (
            <Empty title="スクリーンショットは未取得です">
              <p>
                現在のバージョンには保存済みスクリーンショットを読み込む機能がありません。ゲームの画像は削除されていません。
              </p>
            </Empty>
          )}
        </div>
      </AppShell>
      {dialog.instanceOpen && launcher.selected && (
        <InstanceDialog
          key={launcher.selected.id}
          launcher={launcher}
          initialTab={dialog.initialTab}
          rememberTab={dialog.rememberTab}
          scrollMemory={dialog.scrollMemory}
          onClose={dialog.closeInstance}
        />
      )}
      {launcher.showCreator && <CreateDialog launcher={launcher} onCreated={dialog.onCreated} />}
      {launcher.showAuth && <AuthDialog launcher={launcher} />}
    </>
  );
}
