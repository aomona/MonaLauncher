import { useId, useState } from "react";
import { permissionGroups, unavailableReason } from "./permissionDefinitions";
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
    saving !== null ||
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
      {permissionGroups.map((group) => (
        <section key={group.label} aria-label={group.label} className="mt-6">
          <h4 className="text-navigation">{group.label}</h4>
          {group.items.map((item) => {
            const { key, label, description } = item;
            const unavailable = unavailableReason(item, l.permissionSupport);
            const Label = unavailable ? "span" : "label";
            const incompatible =
              unavailable &&
              l.permissionSupport?.editable &&
              (key === "audioOutput" ? !instance.permissions[key] : instance.permissions[key]);
            const dependency =
              item.file && !instance.permissions.gameWrite
                ? "ゲーム全体の書き込みがOFFのため、読み取り専用です。"
                : key === "microphone" &&
                    !instance.permissions.audioOutput &&
                    !instance.permissions.microphone
                  ? "通常音声をONにすると変更できます。"
                  : key === "audioOutput" && instance.permissions.microphone
                    ? "通常音声をOFFにするには、先にマイクをOFFにしてください。"
                    : null;
            return (
              <div key={key} className="setting-row flex-col shell:flex-row">
                <div className="min-w-0 flex-1">
                  <Label
                    id={`${id}-${key}-label`}
                    htmlFor={unavailable ? undefined : `${id}-${key}`}
                    className="text-navigation"
                  >
                    {label}
                  </Label>
                  <p
                    id={`${id}-${key}-description`}
                    className="mt-1 text-small text-text-secondary"
                  >
                    {description}
                    {dependency && <span className="block mt-1">{dependency}</span>}
                    {unavailable && <span className="block mt-1">{unavailable}</span>}
                  </p>
                  {incompatible && (
                    <div className="mt-2 text-small">
                      <p>別のOSの設定が残っているため、このままでは起動できません。</p>
                      <Button
                        disabled={disabled}
                        onClick={() => void save(key, key === "audioOutput")}
                      >
                        {key === "audioOutput"
                          ? "通常音声を許可に戻す"
                          : `${label}の追加許可を解除`}
                      </Button>
                    </div>
                  )}
                  {failed?.key === key && (
                    <div
                      id={`${id}-${key}-error`}
                      className="mt-2 flex flex-wrap items-center gap-2 text-small"
                    >
                      <span>保存できなかったため、元の設定を維持しています。</span>
                      <Button disabled={disabled} onClick={() => void save(key, failed.value)}>
                        再試行
                      </Button>
                    </div>
                  )}
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  <span className="text-small" aria-hidden="true">
                    {unavailable
                      ? "個別制御未対応"
                      : item.file && !instance.permissions.gameWrite
                        ? "読み取り専用"
                        : instance.permissions[key]
                          ? "許可"
                          : "不許可"}
                  </span>
                  {!unavailable && (
                    <Switch
                      id={`${id}-${key}`}
                      aria-labelledby={`${id}-${key}-label`}
                      aria-describedby={`${id}-${key}-description${failed?.key === key ? ` ${id}-${key}-error` : ""}`}
                      aria-invalid={failed?.key === key || undefined}
                      checked={instance.permissions[key]}
                      disabled={disabled || Boolean(dependency)}
                      onCheckedChange={(value) => void save(key, value)}
                    />
                  )}
                </div>
              </div>
            );
          })}
        </section>
      ))}
      <h3 className="mt-8 text-section-title">固定の権限</h3>
      <p className="mt-2 text-small text-text-secondary">以下は現在変更できません。</p>
      <dl className="mt-2">
        <div className="setting-row">
          <dt>Java・ライブラリ・アセット</dt>
          <dd className="text-small">読み取り専用</dd>
        </div>
        <div className="setting-row">
          <dt>画面・キーボード・マウス</dt>
          <dd className="text-small">許可</dd>
        </div>
      </dl>
      {l.permissionSupport?.platform === "windows" && (
        <p className="mt-4 text-small text-text-secondary">
          Windowsでは互換性のため、起動用ファイルへの書き込みと、バージョン領域・当該インスタンスの管理情報の読み取りも許可します。
        </p>
      )}
      {l.permissionSupport?.platform === "linux" && (
        <p className="mt-4 text-small text-text-secondary">
          LinuxではWayland接続を優先し、Wayland使用時はX11接続を公開しません。X11/XWayland使用時は同じ画面の他アプリへの操作も可能です。PulseAudio互換サーバーへの接続は録音機能へのアクセスも含みます。ナレーター設定はランチャーの読み上げブローカーに適用します。
        </p>
      )}
    </>
  );
}
