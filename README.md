# MonaLauncher

Windows AppContainer内でMinecraftを実行する、開発中のランチャーです。

## Microsoft認証の開発設定

MonaLauncherはデスクトップのpublic clientとしてMicrosoft Device Code Flowを使用します。クライアントシークレットは使用しません。

1. Microsoft Entraでアプリを登録し、個人用Microsoftアカウントを対象に含めます。
2. 「パブリック クライアント フローを許可する」を有効にします。
3. アプリケーション（クライアント）IDを環境変数へ設定してからビルドします。

```powershell
$env:MONALAUNCHER_MICROSOFT_CLIENT_ID = "00000000-0000-0000-0000-000000000000"
mise run dev
```

クライアントIDは公開情報で、ビルドへ埋め込まれます。Microsoftのアクセストークン、device code、更新トークンはReactへ返しません。更新トークンは現在のWindowsユーザーの資格情報マネージャーへ保存します。

認証プロトコルについては、[Microsoft Device Code Flow](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)を参照してください。

現在はMicrosoft OAuthの認証と安全な保存まで実装済みです。Minecraft Servicesへの交換とMinecraftプロフィール取得には、そのサービスで利用可能なアプリ登録が必要で、次の実装段階で起動処理へ接続します。
