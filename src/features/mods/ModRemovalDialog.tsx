import type { Launcher } from "../../app/useLauncher";
import { Button } from "../../components/Button";
import { Dialog } from "../../components/Dialog";
import { ErrorMessage } from "../../components/ErrorMessage";

type ModRemovalDialogModel = Pick<
  Launcher,
  | "isRunning"
  | "modError"
  | "modOperationActive"
  | "modRemovalTarget"
  | "modRemovingProjectId"
  | "removeModrinthMod"
  | "setModRemovalTarget"
>;
export function ModRemovalDialog({ launcher: l }: { launcher: ModRemovalDialogModel }) {
  const target = l.modRemovalTarget;
  if (!target) return null;
  return (
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
        <p className="wrap-anywhere">{target.title}</p>
        <p className="mt-4 text-text-secondary">
          不要になった必須依存も削除されます。他のModが必要とする依存は残ります。
        </p>
        <ErrorMessage>{l.modError}</ErrorMessage>
      </div>
    </Dialog>
  );
}
