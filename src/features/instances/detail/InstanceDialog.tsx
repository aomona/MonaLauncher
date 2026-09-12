import { useEffect, useRef, useState } from "react";
import type { Launcher } from "../../../app/useLauncher";
import { Dialog } from "../../../components/Dialog";
import { Empty } from "../../../components/Empty";
import { ErrorMessage } from "../../../components/ErrorMessage";
import { Tabs } from "../../../components/Tabs";
import { createToastManager, ToastProvider, ToastViewport } from "../../../components/Toast";

import { ModCatalog } from "../../mods/ModCatalog";
import { ModRemovalDialog } from "../../mods/ModRemovalDialog";
import { ModsPanel } from "../../mods/ModsPanel";
import { LogPanel } from "../log/LogPanel";
import { InstanceActionConfirmation, type InstanceAction } from "./InstanceActionConfirmation";
import { InstanceLaunchControls } from "./InstanceLaunchControls";
import { InstanceSettingsPanel } from "./InstanceSettingsPanel";
import { PermissionsPanel } from "./PermissionsPanel";
import { OverviewPanel } from "./OverviewPanel";
import { UnsavedChangesDialog } from "./UnsavedChangesDialog";
import { useInstanceEditor } from "./useInstanceEditor";
import { VersionPanel } from "./VersionPanel";
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
  "Permissions",
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
  const [confirm, setConfirm] = useState<InstanceAction | null>(null);
  const body = useRef<HTMLDivElement>(null);
  const [toasts] = useState(createToastManager);
  const notifySaved = (title: string) => {
    toasts.notify({ id: "instance-save", title, type: "success", priority: "low", timeout: 5000 });
  };
  const notifySaveError = () => {
    toasts.notify({
      id: "instance-save",
      title: "保存できませんでした",
      type: "error",
      priority: "high",
      timeout: 0,
    });
  };
  const editor = useInstanceEditor(l, () => notifySaved("表示名を保存しました"), notifySaveError);
  const changeTab = (next: string) =>
    editor.request(() => {
      setTab(next);
      rememberTab(next);
    });
  useEffect(() => {
    if (body.current) body.current.scrollTop = scrollMemory[`${instance.id}:${tab}`] ?? 0;
  }, [instance.id, tab, scrollMemory]);
  return (
    <ToastProvider toastManager={toasts} timeout={5000} limit={3}>
      <Dialog
        title={instance.name}
        large
        onClose={() => editor.request(onClose)}
        footer={
          <>
            <InstanceLaunchControls launcher={l} onForceQuit={() => setConfirm("stop")} />
            <ToastViewport />
          </>
        }
      >
        <Tabs
          id="instance"
          tabs={instanceTabs}
          active={tab}
          onChange={changeTab}
          panelProps={{
            ref: body,
            className: `dialog-body flex-1 ${tab === "Log" ? "flex flex-col" : ""}`,
            onScroll: (event) => {
              scrollMemory[`${instance.id}:${tab}`] = event.currentTarget.scrollTop;
            },
          }}
        >
          <ErrorMessage>{l.error}</ErrorMessage>
          {!instance.sandboxed && (
            <ErrorMessage>
              旧形式のため起動できません。新しいインスタンスを作成してください。
            </ErrorMessage>
          )}
          {tab === "Overview" && (
            <OverviewPanel launcher={l} onRepair={() => setConfirm("repair")} />
          )}
          {tab === "Log" && <LogPanel entries={l.visibleLogs} />}
          {tab === "Version" && <VersionPanel instance={instance} />}
          {tab === "Mods" && <ModsPanel launcher={l} />}
          {tab === "Permissions" && (
            <PermissionsPanel
              launcher={l}
              onSaved={() => notifySaved("権限を保存しました")}
              onSaveError={notifySaveError}
            />
          )}
          {tab === "Settings" && (
            <InstanceSettingsPanel
              launcher={l}
              editor={editor}
              onDelete={() => setConfirm("delete")}
            />
          )}
          {["Resource Packs", "Shader Packs", "Worlds", "Servers", "Screenshots"].includes(tab) && (
            <Empty title={`${tab}は未取得です`}>
              <p>
                現在のアプリはこのデータの読み込み・管理に対応していません。保存済みファイルの件数や状態は確認できていません。
              </p>
            </Empty>
          )}
        </Tabs>
        {editor.guard && <UnsavedChangesDialog launcher={l} editor={editor} returnFocus={body} />}
        {confirm && (
          <InstanceActionConfirmation
            key={confirm}
            launcher={l}
            action={confirm}
            onCancel={() => setConfirm(null)}
            onClose={onClose}
          />
        )}
        {l.showMods && <ModCatalog launcher={l} />}
        {l.modRemovalTarget && <ModRemovalDialog launcher={l} />}
      </Dialog>
    </ToastProvider>
  );
}
