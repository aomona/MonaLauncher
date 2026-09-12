import { useId, useState } from "react";
import type { Launcher } from "../../../app/useLauncher";
import { Button } from "../../../components/Button";
import { ErrorMessage } from "../../../components/ErrorMessage";
import { Switch } from "../../../components/Switch";
import type { InstancePermissions } from "../../../domain/launcher";

type Model = Pick<
  Launcher,
  | "selected"
  | "busy"
  | "isRunning"
  | "modOperationActive"
  | "permissionSupport"
  | "permissionSupportError"
  | "refreshPermissionSupport"
>;
const permissions = [
  {
    key: "gameWrite",
    label: "ゲームデータへの書き込み",
    description:
      "このインスタンスのワールド・設定・ログなどを保存します。無効にすると保存できず、ゲームが起動しない場合もあります。",
  },
  {
    key: "narrator",
    label: "ナレーター",
    description:
      "ゲームとModからのテキスト読み上げを許可します。通常の効果音や音楽には影響しません。",
  },
] as const;

export function PermissionsPanel({
  launcher: l,
  onSave,
}: {
  launcher: Model;
  onSave: (permissions: InstancePermissions) => Promise<boolean>;
}) {
  const id = useId();
  const [saving, setSaving] = useState<keyof InstancePermissions | null>(null);
  const [failed, setFailed] = useState<{ key: keyof InstancePermissions; value: boolean } | null>(
    null,
  );
  const instance = l.selected!;
  const disabled =
    Boolean(l.busy) ||
    l.isRunning ||
    l.modOperationActive ||
    !instance.sandboxed ||
    !l.permissionSupport?.editable;
  const save = async (key: keyof InstancePermissions, value: boolean) => {
    if (disabled || saving) return;
    setSaving(key);
    setFailed(null);
    const ok = await onSave({ ...instance.permissions, [key]: value });
    setSaving(null);
    if (!ok) setFailed({ key, value });
  };
  return (
    <>
      <h3 className="text-section-title">Permissions</h3>
      <p className="mt-2 text-small text-text-secondary">
        このインスタンスのゲームとすべてのModに適用します。変更は自動保存され、次回起動から有効です。
      </p>
      {l.isRunning && (
        <output className="block mt-3 text-small">
          権限を変更するにはゲームを終了してください。
        </output>
      )}
      {l.modOperationActive && (
        <output className="block mt-3 text-small">Modの処理が完了してから変更できます。</output>
      )}
      {!l.permissionSupport && !l.permissionSupportError && (
        <output className="block mt-3 text-small">権限の対応状況を確認しています…</output>
      )}
      {l.permissionSupportError && (
        <ErrorMessage>
          <p>権限の対応状況を取得できませんでした。{l.permissionSupportError}</p>
          <Button onClick={() => void l.refreshPermissionSupport()}>再試行</Button>
        </ErrorMessage>
      )}
      {l.permissionSupport && !l.permissionSupport.editable && (
        <p className="mt-3 text-small">このOSの権限設定はまだ未対応です。</p>
      )}
      {permissions.map(({ key, label, description }) => (
        <div key={key} className="setting-row flex-col shell:flex-row">
          <div className="min-w-0 flex-1">
            <label id={`${id}-${key}-label`} htmlFor={`${id}-${key}`} className="text-navigation">
              {label}
            </label>
            <p id={`${id}-${key}-description`} className="mt-1 text-small text-text-secondary">
              {description}
            </p>
            {failed?.key === key && (
              <div className="mt-2 flex flex-wrap items-center gap-2 text-small">
                <span>保存できなかったため、元の設定を維持しています。</span>
                <Button disabled={disabled} onClick={() => void save(key, failed.value)}>
                  再試行
                </Button>
              </div>
            )}
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <span className="text-small" aria-hidden="true">
              {instance.permissions[key] ? "許可" : "不許可"}
            </span>
            <Switch
              id={`${id}-${key}`}
              aria-labelledby={`${id}-${key}-label`}
              aria-describedby={`${id}-${key}-description`}
              checked={instance.permissions[key]}
              disabled={disabled}
              onCheckedChange={(value) => void save(key, value)}
            />
          </div>
        </div>
      ))}
      <h3 className="mt-8 text-section-title">固定の権限</h3>
      <p className="mt-2 text-small text-text-secondary">以下は現在変更できません。</p>
      <dl className="mt-2">
        <div className="setting-row">
          <dt>ゲームのネットワーク通信</dt>
          <dd className="text-small">不許可</dd>
        </div>
        <div className="setting-row">
          <dt>Java・ライブラリ・アセット</dt>
          <dd className="text-small">読み取り専用</dd>
        </div>
        <div className="setting-row">
          <dt>画面・キーボード・マウス・通常音声</dt>
          <dd className="text-small">許可</dd>
        </div>
      </dl>
      {l.permissionSupport?.platform === "windows" && (
        <p className="mt-4 text-small text-text-secondary">
          Windowsでは互換性のため、起動用ファイルへの書き込みと、バージョン領域・当該インスタンスの管理情報の読み取りも許可します。
        </p>
      )}
    </>
  );
}
