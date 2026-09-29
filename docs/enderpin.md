# Enderpin連携

MonaLauncherはEnderpinのRustライブラリを直接リンクする。ユーザーによるCLIのインストール、PATH設定、別プロセスの起動は不要。依存先は `src-tauri/Cargo.toml` のGitコミットと `Cargo.lock` で固定する。

## 利用範囲

- `enderpin::runtime` の `Arguments` / `Argument` / `Rule` をメタデータとして読み、`rules_allow` でライブラリ・nativeの選択と診断を、`arguments` でMinecraftとFabricのJVM・ゲーム引数を評価する。旧形式の `minecraftArguments` も分割後に同じ評価へ渡す。
- `enderpin::registry::response_with_policy` でMinecraftのカタログ・メタデータ・本体・ライブラリ・アセット、およびFabricライブラリ・Javaアーカイブを取得する。EnderpinがHTTPS、DNSの固定、公開IPの検査、接続再利用、タイムアウト、リダイレクトを処理する。
- 配布元の許可リストは最初のURLと**各リダイレクト先への接続前**に適用する。ファイルごとのサイズ・ハッシュ、取得計画の件数・総量、アーカイブ展開の制限、原子的な置換はMonaLauncher側で維持する。Fabric/Adoptiumのカタログは既存のHTTPステータス処理を使う。

Enderpinは不明なルールaction、対象OSに対する未対応のOSバージョン条件、NULを含む引数、過大な引数をエラーにする。エラーは導入・診断・起動の呼び出し元へ返し、旧評価器や非サンドボックス起動へ切り替えない。

## MonaLauncherが管理するもの

`instance.json`、保存先、既存のワールド・Mod管理、Javaの選択・展開、プラットフォーム固有のnative補正、権限設定、認証仲介、ナレーター、OSサンドボックス、プロセスの監視・終了は既存のMonaLauncher実装を使う。IPCとフロントエンドの型は変更しない。

Enderpinの `Workspace` / `launch::start` への全面移行ではない。それらの設定・ディレクトリ・認証モデルへ既存インスタンスを変換せず、組み込み用APIを現在の導入・起動経路から利用する。ゲームの通信は既定OFF、アカウント認証の仲介は別設定で、実トークンや署名秘密鍵をゲームへ渡さない。

## 検証

```sh
mise run rust-format
mise run rust-lint
mise run rust-test
```

通常テストはEnderpinを通る引数展開、認証用プレースホルダー、ルールエラー、URL拒否、応答サイズ制限と既存の権限・認証・プロセス管理を検証する。次の追加テストは公式HTTPS配布元へ接続し、Minecraft 1.21.8のメタデータと小さなライブラリを取得して破損修復・ハッシュ不一致時の保持を確認する。アカウントは使用しない。

```sh
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib minecraft::installer::tests::live_enderpin_metadata_and_verified_download -- --ignored --exact
```

実ゲームの確認は [macOS](macos-seatbelt.md)、[Linux](../tools/linux-validation/README.md)、[Windowsを含むCI](ci.md) の手順で行う。単体テスト・HTTPS取得・別OSでの成功を、そのOSの実ゲームや認証接続の成功とは扱わない。

### 2026-09-30の確認結果

- macOSで `mise run rust-format` / `rust-lint` / `rust-test` が成功。Rustテスト142件成功、外部サービス・アカウント等を使う10件は通常実行から除外。`pnpm check` も成功。
- 上記の公式HTTPS取得テストを個別に実行し、メタデータ取得、Java majorの解決、ライブラリ取得・破損修復、ハッシュ不一致時の既存ファイル保持を確認。
- `MONALAUNCHER_EXPECT_NARRATOR=1` と既存の `minecraft_smoke` でMinecraft 26.2を25秒間確認。Seatbelt下でのプロセス起動・生存、音声エンジン初期化、ナレーターの開始・完了各1回、終了を確認した。ただしウィンドウは検出されたものの `onScreen=false` で、CLIは終了コード1。画面表示を含むスモーク全体は未成功。
- Windows/Linuxの実ゲーム、今回の変更後の実アカウント認証、ランチャーUIからのPlay操作は未検証。
