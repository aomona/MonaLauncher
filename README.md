# MonaLauncher

Windows AppContainer内でMinecraftを実行する、開発中のランチャーです。ゲームプロセスにはネットワークCapabilityを付与せず、インスタンス専用のファイル領域だけへアクセスを限定します。

macOSではSeatbeltによる実験的な起動にも対応しています。Minecraft 1.21.8デモ版のタイトル画面まで確認済みです。[macOSの起動手順・制限・検証結果](docs/macos-seatbelt.md)を参照してください。

## Windowsのセキュリティ境界

- MinecraftとModは専用AppContainer SIDで実行し、ネットワークCapabilityを付与しません。そのため、現状はマルチプレイ、Realmsなどゲーム側の通信機能を利用できません。
- 共有ライブラリ、Java、起動メタデータ、インスタンス設定は読み取り専用です。書き込みを許可するのはゲームデータと起動ごとの一時ディレクトリだけです。
- ゲームデータ内のジャンクションやシンボリックリンクをホスト側のMod管理に利用できないよう、再解析ポイントを拒否します。
- 子プロセスはkill-on-close Job Objectへ、停止状態のまま割り当ててから開始します。通常終了だけでなく、ランチャーが異常終了した場合もプロセスツリーを残しません。
- OAuthのdevice code、更新トークン、アクセストークンはReactやゲーム／Modプロセスへ渡しません。更新トークンはWindows資格情報マネージャーへ保存します。外部応答、アーカイブ、ダウンロード、ローカル設定には件数・サイズ・パス・配布元の検証を行います。

実装上の境界は次のコマンドで回帰確認できます。

```powershell
pnpm check
cd src-tauri
cargo test --all-targets --all-features --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo run --locked --bin sandbox_probe -- --appcontainer
cargo run --locked --bin sandbox_probe -- --acl
```

## Microsoft認証の開発設定

MonaLauncherはデスクトップのpublic clientとしてMicrosoft Device Code Flowを使用します。クライアントシークレットは使用しません。

1. Microsoft Entraでアプリを登録し、個人用Microsoftアカウントを対象に含めます。
2. 「パブリック クライアント フローを許可する」を有効にします。
3. アプリケーション（クライアント）IDをMonaLauncherの既定IDとして設定します。

```powershell
$env:MONALAUNCHER_MICROSOFT_CLIENT_ID = "別の開発用クライアントID"
mise run dev
```

既定ではMonaLauncher用の公開クライアントID `f8d68570-e721-4aba-9c3e-1052d41e431a` がビルドへ組み込まれているため、環境変数の設定は不要です。環境変数は別のアプリ登録で開発するときの上書き用です。Microsoftのアクセストークン、device code、更新トークンはReactへ返さず、アクセストークンはゲーム／Modにも渡しません。更新トークンはWindowsでは現在のユーザーの資格情報マネージャー、macOSではキーチェーンへ保存します。Linuxのトークン保存は未対応です。

ランチャーのアカウント設定からサインインを開始し、表示されたコードをブラウザーのMicrosoft認証画面で入力してください。macOSでは保存・読込・サインアウト時の削除にSecurity.frameworkを使用し、保存先を平文ファイルへ切り替えることはありません。

認証プロトコルについては、[Microsoft Device Code Flow](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)を参照してください。

Microsoft OAuth、Xbox Live、XSTS、Minecraft Services、Minecraftプロフィール取得までを実装しています。Microsoftアプリ登録がMinecraft Servicesで利用できない場合は、認証時に`Invalid app registration`が返されます。

macOSでの認証接続・保存先の実機確認:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked auth:: -- --include-ignored
```

この確認はMicrosoftからの認証コード取得とログイン待ち応答、キーチェーンの専用テスト項目の保存・更新・読込・削除を検証します。認証コードやトークンを出力・ファイル保存せず、既存のサインイン情報には触れません。2026-09-12に上記の既定IDとmacOSで成功を確認しました。ユーザーによるサインイン完了とMinecraft Servicesの利用可否は、このテストの確認範囲に含まれません。
