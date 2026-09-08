import type { Launcher } from "../../app/useLauncher";
import { Button, ErrorMessage } from "../../components/ui";

import { hasTauriRuntime } from "../../lib/tauri";

type AccountSettingsModel = Pick<
  Launcher,
  "authError" | "authStatus" | "minecraftProfile" | "minecraftProfileLoading" | "openAuth"
>;
export function AccountSettings({ launcher }: { launcher: AccountSettingsModel }) {
  const native = hasTauriRuntime();
  return (
    <>
      <h2 className="mt-8 text-section-title">Accounts</h2>
      <div className="setting-row">
        <div>
          <p className="text-navigation">
            {launcher.minecraftProfile?.name ?? "Microsoft account"}
          </p>
          <p className="mt-1 text-small text-text-secondary">
            {launcher.minecraftProfileLoading
              ? "プロフィールを確認しています…"
              : launcher.authStatus.authorized
                ? "Microsoft認証済み"
                : launcher.authStatus.configured
                  ? "未設定 · サインインできます"
                  : "Microsoft認証の構成が必要です"}
          </p>
        </div>
        <Button disabled={!native} onClick={launcher.openAuth}>
          {launcher.authStatus.authorized ? "アカウントを管理" : "Sign in"}
        </Button>
      </div>
      <ErrorMessage>{launcher.authError}</ErrorMessage>
    </>
  );
}
