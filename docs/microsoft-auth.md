# Microsoft認証の開発設定

MonaLauncherはデスクトップのpublic clientとしてMicrosoft Device Code Flowを使用します。クライアントシークレットは使用しません。

## 通常のサインイン

既定ではMonaLauncher用の公開クライアントID `f8d68570-e721-4aba-9c3e-1052d41e431a` がビルドへ組み込まれているため、環境変数の設定や独自のアプリ登録は不要です。

ランチャーのアカウント設定からサインインを開始し、表示されたコードをブラウザーのMicrosoft認証画面で入力してください。

Microsoft OAuth、Xbox Live、XSTS、Minecraft Services、Minecraftプロフィール取得までを実装しています。Microsoftアプリ登録がMinecraft Servicesで利用できない場合は、認証時に`Invalid app registration`が返されます。

## 別のクライアントIDを使う場合

1. Microsoft Entraでアプリを登録し、個人用Microsoftアカウントを対象に含めます。
2. 「パブリック クライアント フローを許可する」を有効にします。
3. アプリケーション（クライアント）IDを環境変数に設定して起動します。

PowerShell:

```powershell
$env:MONALAUNCHER_MICROSOFT_CLIENT_ID = "別の開発用クライアントID"
mise run dev
```

macOSなどのシェル:

```sh
MONALAUNCHER_MICROSOFT_CLIENT_ID="別の開発用クライアントID" mise run dev
```

認証プロトコルについては、[Microsoft Device Code Flow](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)を参照してください。

## 認証情報とゲームへの仲介

Microsoftのアクセストークン、device code、更新トークンはReactやゲーム／Modプロセスへ渡しません。Minecraftアクセストークンとチャット秘密鍵もRust側に保持し、ゲーム／Modには渡しません。

インスタンスのPermissionsで「アカウント認証の仲介」をONにすると、ゲームには固定の無効トークンと起動ごとのIPCを渡し、必要な認証・署名操作をランチャーが仲介します。既定はOFFで、デモ・未ログイン時は対象外です。ゲーム自身のネットワーク通信は別の権限です。

認証仲介はMinecraft 1.21.8・26.2の指定構成で試験対応しています。Fabricは0.19.5が対象で、Java・authlibなどの構成も検証し、未対応の組み合わせは拒否します。詳細は[認証仲介の設計と検証](auth-broker-plan.md)を参照してください。

更新トークンはWindowsでは現在のユーザーの資格情報マネージャー、macOSではキーチェーンへ保存します。macOSでは保存・読込・サインアウト時の削除にSecurity.frameworkを使用し、保存先を平文ファイルへ切り替えることはありません。Linuxのトークン保存は未対応です。

## macOSでの認証接続・保存先の確認

次のテストはMicrosoftへの接続と、キーチェーンの専用テスト項目の操作を行います。通常のテストでは除外されています。

```sh
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked auth::microsoft::tests::live_device_authorization_reaches_pending -- --ignored --exact
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked auth::token_store::tests::macos_keychain_round_trip -- --ignored --exact
```

Microsoftからの認証コード取得とログイン待ち応答、キーチェーンの専用テスト項目の保存・更新・読込・削除を検証します。認証コードやトークンを出力・ファイル保存せず、既存のサインイン情報には触れません。

旧READMEには、2026-09-12に上記の既定IDとmacOSで成功した記録があります。ユーザーによるサインイン完了とMinecraft Servicesの利用可否は、これらのテストの確認範囲に含まれません。

## 一覧からの今回限りの起動モード

通常のPlayは保存済みデモ設定とアカウント状態に従う。一覧Menuのオフライン起動は認証ストア・セッション更新・認証仲介を使わず通常モードを、デモ起動は同じく認証なしでデモモードを起動する。どちらも今回限りで、保存済みモードやPermissionsを変更しない。オフラインはゲームの通信遮断を意味せず、通信はPermissionsの設定に従う。IPCのmodeを省略した場合は従来のPlayと同じ扱い。

オフライン起動のIPCにはofflineUsernameが必須。UIの入力DialogとRust側の両方で半角英数字・_の1〜16文字を検証する。他モードでの名前指定は拒否する。起動処理の最終段でもオフライン用の名前だけを取り出して検証し、アカウントのUUIDや認証ブローカーを引き継がない。UUIDはMinecraftと同じくUTF-8の`OfflinePlayer:<ユーザー名>`からJavaの`UUID.nameUUIDFromBytes`相当で生成する。同じ名前では同じUUID、異なる名前では別のプレイヤーとして扱う。ユーザー名は今回の起動引数へ渡し、インスタンスの保存内容やMicrosoftアカウントは変更しない。
