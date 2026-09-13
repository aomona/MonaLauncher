# CIとリリース前の確認

`CI` はPR、`dev` / `main` へのpush、手動実行で動く。作業ブランチへのpushとPR更新の二重実行を避け、PRを作る前はActionsの `Run workflow` でブランチを指定する。`Project checks` は下記ジョブすべての成功を要求する集約チェックとして名前を維持する。

| ジョブ                     | 確認内容                                                                                                                                               |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Frontend and UI            | デザイントークン、整形、lint、型検査、RSS生成テスト、Viteビルド、Playwrightによる画面操作・RSS XML検証                                                 |
| Rust and sandbox (Windows) | Rustの整形・Clippy・単体テスト、生成RSSのRust側読み込み、ネイティブビルド、AppContainerトークン、用途別ACLの拒否と復帰、親の異常終了時の子プロセス終了 |
| Rust and sandbox (macOS)   | 同じRust・RSS・ビルド検証に加え、単体テスト内で本番Seatbelt経路のファイル・通信・サービス権限、子への制限継承、キャッシュの権限切り替えを実行          |
| Rust and sandbox (Linux)   | 同じRust・RSS・ビルド検証に加え、本番bubblewrap経路のファイル・通信・seccomp・子への制限継承・終了処理を非rootで実行                                   |

Rust依存のビルドキャッシュをOSとツールチェーンごとに再利用する。UIテストに失敗した場合は `browser-test-results` にスクリーンショットとトレースを7日間保存する。

Linuxでは検証バイナリをroot所有の専用ディレクトリへコピーし、AppArmorが有効なら既存の検証用プロファイルでuser namespaceの使用を許可する。実行自体は通常ユーザーで行う。ホスト全体のAppArmorやnamespace制限を無効にしない。サンドボックスの起動失敗は検証失敗となり、スキップや通常起動へのフォールバックはしない。

生成RSSのRust読み込みテストは、`pnpm build` の後に対象テストだけ `--ignored --exact` で実行する。他のignoredテスト（Microsoft認証・公開ニュースサービス・個人のキーチェーン）を一括で有効にしない。

## ローカルでの実行

```sh
mise install
mise exec -- pnpm install --frozen-lockfile
mise exec -- pnpm check
mise exec -- pnpm exec playwright install chromium
mise exec -- pnpm test:ui
mise run rust-format
mise run rust-lint
mise run rust-test
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib commands::news::launcher::tests::generated_launcher_feed -- --ignored --exact
cargo build --manifest-path src-tauri/Cargo.toml --locked --bins --features tauri/custom-protocol
```

Windowsの追加検証は `sandbox_probe.exe --appcontainer`、`sandbox_probe.exe --acl`、`scripts/verify-windows-job.ps1`。Linuxの必要パッケージとプロファイル設定は [Linux検証手順](../tools/linux-validation/README.md) を参照。

## リリース候補で別途確認すること

通常CIの成功だけでは、配布パッケージや実ゲームの動作確認にはならない。各OSのリリース候補について以下を記録する。

- `mise run build` で配布物を生成し、開発用サーバーがない環境へインストールして起動する。
- Microsoftログインと認証情報の保存・復元を専用の検証アカウントで確認する。
- Minecraftを起動し、GPU描画・通常音声・ナレーターを確認する。LinuxはX11とWaylandを分ける。
- ランチャー上で権限を保存し、次回起動への反映と再許可、停止・ランチャー終了後の子プロセスの後始末を確認する。

WindowsのCIは通信ON/OFFの実接続を検証していない。Linuxの通常CIはデスクトップサービス・実GPUを検証していない。macOSのマイク検証はポリシー判定であり、TCCの許可や実録音の確認ではない。これらをCIの成功から推定しない。
