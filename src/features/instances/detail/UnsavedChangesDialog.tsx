import type { Launcher } from "../../../app/useLauncher";
import { Button, Dialog, ErrorMessage } from "../../../components/ui";

import type { InstanceEditor } from "./useInstanceEditor";
export function UnsavedChangesDialog({
  launcher: l,
  editor,
}: {
  launcher: Launcher;
  editor: InstanceEditor;
}) {
  return (
    <Dialog
      title="未保存の変更があります"
      onClose={editor.cancelNavigation}
      footer={
        <>
          <Button data-initial-focus onClick={editor.cancelNavigation}>
            編集を続ける
          </Button>
          <Button disabled={Boolean(l.busy)} onClick={editor.discardAndContinue}>
            破棄して移動
          </Button>
          <Button
            tone="primary"
            disabled={Boolean(l.busy) || !l.settingsName.trim()}
            onClick={() => void editor.saveAndContinue()}
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
  );
}
