import { useState } from "react";
import type { Launcher } from "../../../app/useLauncher";
import { Button } from "../../../components/Button";
import { Dialog } from "../../../components/Dialog";
import { ErrorMessage } from "../../../components/ErrorMessage";

export type InstanceAction = "stop" | "delete" | "repair";
export function InstanceActionConfirmation({
  launcher: l,
  action: confirm,
  onCancel,
  onClose,
}: {
  launcher: Launcher;
  action: InstanceAction;
  onCancel: () => void;
  onClose: () => void;
}) {
  const instance = l.selected!;
  const [deleteName, setDeleteName] = useState("");
  return (
    <Dialog
      title={
        confirm === "stop"
          ? "Minecraftを強制終了しますか？"
          : confirm === "delete"
            ? "インスタンスを削除しますか？"
            : "管理対象ファイルを修復しますか？"
      }
      onClose={() => {
        if (!l.busy) onCancel();
      }}
      footer={
        <>
          <Button data-initial-focus disabled={Boolean(l.busy)} onClick={() => onCancel()}>
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
                onCancel();
                void l.stop();
              } else if (confirm === "repair") {
                onCancel();
                void l.repairSelected();
              } else
                void l.deleteSelected().then((ok) => {
                  if (ok) {
                    onCancel();
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
              <input value={deleteName} onChange={(event) => setDeleteName(event.target.value)} />
            </label>
          </>
        )}
        <ErrorMessage>{l.error}</ErrorMessage>
      </div>
    </Dialog>
  );
}
