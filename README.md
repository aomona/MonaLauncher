# MonaLauncher

Windows AppContainer内でMinecraftを実行する、開発中のランチャーです。

## Microsoft認証の開発設定

MonaLauncherはデスクトップのpublic clientとしてMicrosoft Device Code Flowを使用します。クライアントシークレットは使用しません。

1. Microsoft Entraでアプリを登録し、個人用Microsoftアカウントを対象に含めます。
2. 「パブリック クライアント フローを許可する」を有効にします。
3. アプリケーション（クライアント）IDをMonaLauncherの既定IDとして設定します。

```powershell
$env:MONALAUNCHER_MICROSOFT_CLIENT_ID = "別の開発用クライアントID"
mise run dev
```

既定ではMonaLauncher用のクライアントIDがビルドへ組み込まれているため、環境変数の設定は不要です。環境変数は別のアプリ登録で開発するときの上書き用です。クライアントIDは公開情報です。Microsoftのアクセストークン、device code、更新トークンはReactへ返しません。更新トークンは現在のWindowsユーザーの資格情報マネージャーへ保存します。

認証プロトコルについては、[Microsoft Device Code Flow](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)を参照してください。

Microsoft OAuth、Xbox Live、XSTS、Minecraft Services、Minecraftプロフィール取得までを実装しています。Microsoftアプリ登録がMinecraft Servicesで利用できない場合は、認証時に`Invalid app registration`が返されます。
