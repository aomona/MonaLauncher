import type { Launcher } from "../../../app/useLauncher";
import { Button } from "../../../components/Button";
import { CopyButton } from "../../../components/CopyButton";

import type { InstanceEditor } from "./useInstanceEditor";
type InstanceSettingsPanelModel = Pick<
  Launcher,
  "busy" | "isRunning" | "modOperationActive" | "selected" | "setSettingsName" | "settingsName"
>;
export function InstanceSettingsPanel({
  launcher: l,
  editor,
  onDelete,
}: {
  launcher: InstanceSettingsPanelModel;
  editor: InstanceEditor;
  onDelete: () => void;
}) {
  const instance = l.selected!;
  const { draft, save } = editor;
  return (
    <>
      <h3 className="text-section-title">General</h3>
      <div className="py-4">
        <label className="field">
          表示名
          <span className="field-hint">表示名のみを変更します。ディレクトリ名は変わりません。</span>
          <input
            maxLength={80}
            value={l.settingsName}
            disabled={l.busy === "rename"}
            onChange={(event) => {
              l.setSettingsName(event.target.value);
            }}
            onKeyDown={(event) => {
              if (
                event.key === "Enter" &&
                !event.nativeEvent.isComposing &&
                !l.busy &&
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
              Apply
            </Button>
            <Button disabled={Boolean(l.busy)} onClick={() => l.setSettingsName(instance.name)}>
              Cancel
            </Button>
          </div>
        )}
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
            onDelete();
          }}
        >
          削除…
        </Button>
      </div>
    </>
  );
}
