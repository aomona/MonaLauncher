import { useEffect, useRef, useState } from "react";
import type { Launcher } from "../../../app/useLauncher";
import { Dialog, Empty, ErrorMessage, Tabs } from "../../../components/ui";

import { ModCatalog } from "../../mods/ModCatalog";
import { ModRemovalDialog } from "../../mods/ModRemovalDialog";
import { ModsPanel } from "../../mods/ModsPanel";
import { LogPanel } from "../log/LogPanel";
import { InstanceActionConfirmation, type InstanceAction } from "./InstanceActionConfirmation";
import { InstanceLaunchControls } from "./InstanceLaunchControls";
import { InstanceSettingsPanel } from "./InstanceSettingsPanel";
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
  const editor = useInstanceEditor(l);
  const changeTab = (next: string) =>
    editor.request(() => {
      setTab(next);
      rememberTab(next);
    });
  useEffect(() => {
    if (body.current) body.current.scrollTop = scrollMemory[`${instance.id}:${tab}`] ?? 0;
  }, [instance.id, tab, scrollMemory]);
  return (
    <>
      <Dialog
        title={instance.name}
        large
        onClose={() => editor.request(onClose)}
        footer={<InstanceLaunchControls launcher={l} onForceQuit={() => setConfirm("stop")} />}
      >
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
            <OverviewPanel launcher={l} onRepair={() => setConfirm("repair")} />
          )}
          {tab === "Log" && <LogPanel entries={l.visibleLogs} />}
          {tab === "Version" && <VersionPanel instance={instance} />}
          {tab === "Mods" && <ModsPanel launcher={l} />}
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
        </div>
      </Dialog>
      {editor.guard && <UnsavedChangesDialog launcher={l} editor={editor} />}
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
    </>
  );
}
