import { useId, useState } from "react";
import { Button } from "../../components/Button";
import { Dialog } from "../../components/Dialog";
import type { MinecraftInstance } from "../../domain/launcher";

export function OfflineLaunchDialog({
  instance,
  disabled,
  onLaunch,
  onClose,
  finalFocus,
}: {
  instance: MinecraftInstance;
  disabled: boolean;
  onLaunch: (username: string) => void;
  onClose: () => void;
  finalFocus: () => HTMLElement | null;
}) {
  const [username, setUsername] = useState("Player");
  const formId = useId();
  const hintId = useId();
  const valid = /^[A-Za-z0-9_]{1,16}$/.test(username);
  return (
    <Dialog
      title="オフラインモードで起動"
      onClose={onClose}
      finalFocus={finalFocus}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button type="submit" form={formId} tone="primary" disabled={disabled || !valid}>
            起動
          </Button>
        </>
      }
    >
      <form
        id={formId}
        className="dialog-body"
        onSubmit={(event) => {
          event.preventDefault();
          if (!disabled && valid) onLaunch(username);
        }}
      >
        <p className="mb-4 wrap-anywhere font-medium">{instance.name}</p>
        <label className="field">
          ユーザー名
          <input
            data-initial-focus
            value={username}
            onChange={(event) => setUsername(event.target.value)}
            onFocus={(event) => event.target.select()}
            onKeyDown={(event) => {
              if (event.key === "Enter" && event.nativeEvent.isComposing) event.preventDefault();
            }}
            required
            maxLength={16}
            pattern="[A-Za-z0-9_]{1,16}"
            autoComplete="off"
            autoCapitalize="none"
            spellCheck={false}
            aria-invalid={!valid || undefined}
            aria-describedby={hintId}
          />
        </label>
        <p id={hintId} className="mt-2 text-small text-text-secondary">
          半角英数字とアンダースコア（_）を1〜16文字で入力してください。
        </p>
      </form>
    </Dialog>
  );
}
