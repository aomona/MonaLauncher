import { ExternalLink } from "lucide-react";
import type { Launcher } from "../../app/useLauncher";
import { Button } from "../../components/Button";
import { CopyButton } from "../../components/CopyButton";
import { Dialog } from "../../components/Dialog";
import { ErrorMessage } from "../../components/ErrorMessage";

type AuthDialogModel = Pick<
  Launcher,
  | "authBusy"
  | "authChallenge"
  | "authError"
  | "authStatus"
  | "beginMicrosoftSignIn"
  | "minecraftProfile"
  | "minecraftProfileLoading"
  | "openMicrosoftVerification"
  | "refreshMinecraftProfile"
  | "setShowAuth"
  | "signOutMicrosoft"
>;
export function AuthDialog({ launcher: l }: { launcher: AuthDialogModel }) {
  return (
    <Dialog title="Microsoft account" onClose={() => l.setShowAuth(false)}>
      <div className="dialog-body">
        <p className="mb-4 text-text-secondary">
          Microsoftアカウントを使ってMinecraftのプロフィールを確認します。
        </p>
        <ErrorMessage>{l.authError}</ErrorMessage>
        {!l.authStatus.configured ? (
          <p>このビルドにはMicrosoft認証が構成されていません。</p>
        ) : l.authStatus.authorized ? (
          <>
            <p className="text-navigation">{l.minecraftProfile?.name ?? "Microsoft認証済み"}</p>
            <p className="mt-2 text-small">
              {l.minecraftProfileLoading
                ? "Minecraftプロフィールを確認中…"
                : l.minecraftProfile
                  ? "Minecraft: Java Edition"
                  : "Minecraftプロフィールを確認できていません。"}
            </p>
            <div className="mt-6 flex flex-wrap gap-2">
              <Button
                disabled={l.minecraftProfileLoading}
                onClick={() => void l.refreshMinecraftProfile()}
              >
                プロフィールを再確認
              </Button>
              <Button disabled={Boolean(l.authBusy)} onClick={() => void l.signOutMicrosoft()}>
                Sign out
              </Button>
            </div>
          </>
        ) : (
          <>
            <Button
              tone="primary"
              disabled={Boolean(l.authBusy) || Boolean(l.authChallenge)}
              onClick={() => void l.beginMicrosoftSignIn()}
            >
              {l.authBusy ? "準備しています…" : "Microsoftでサインイン"}
            </Button>
            {l.authChallenge && (
              <div className="mt-6">
                <p>ブラウザーで次のコードを入力してください。</p>
                <div className="my-4 flex flex-wrap items-center gap-2">
                  <code className="font-mono text-section-title">{l.authChallenge.userCode}</code>
                  <CopyButton label="認証コード" value={l.authChallenge.userCode} />
                </div>
                <Button
                  onClick={() => void l.openMicrosoftVerification(l.authChallenge!.verificationUri)}
                >
                  <ExternalLink size={16} />
                  ブラウザーを開く
                </Button>
                <p className="mt-2 wrap-anywhere font-mono text-small">
                  {l.authChallenge.verificationUri}
                </p>
                <output className="mt-4 text-small">認証の完了を待っています…</output>
              </div>
            )}
          </>
        )}
      </div>
    </Dialog>
  );
}
