import { VersionSelector } from "./VersionSelector";
import { useRef, useState } from "react";
import type { Launcher } from "../../../app/useLauncher";
import { Button, Dialog, ErrorMessage, Progress } from "../../../components/ui";
import type { MinecraftInstance } from "../../../domain/launcher";

type CreateDialogModel = Pick<
  Launcher,
  | "busy"
  | "creatorError"
  | "fabricLoaders"
  | "fabricLoadersError"
  | "fabricLoadersLoading"
  | "gameMode"
  | "install"
  | "instanceName"
  | "modLoaderType"
  | "progress"
  | "progressPercent"
  | "refreshVersions"
  | "selectedFabricLoader"
  | "selectedVersionId"
  | "setFabricReloadKey"
  | "setGameMode"
  | "setInstanceName"
  | "setModLoaderType"
  | "setSelectedFabricLoader"
  | "setSelectedVersionId"
  | "setShowCreator"
  | "setVersionQuery"
  | "versionCatalog"
  | "versionGroups"
  | "versionQuery"
  | "versionsError"
  | "versionsLoading"
>;
export function CreateDialog({
  launcher: l,
  onCreated,
}: {
  launcher: CreateDialogModel;
  onCreated: (item: MinecraftInstance) => void;
}) {
  const [discard, setDiscard] = useState(false);
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
            <VersionSelector launcher={l} />
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
