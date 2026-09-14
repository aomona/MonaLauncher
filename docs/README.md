# エージェント向け開発ガイド

共通ルールは [AGENTS.md](../AGENTS.md)、環境構築・起動は [README.md](../README.md) を参照する。このページは作業対象から既存資料と実装へ辿るための入口。

## 作業別に読む資料

| 変更対象                        | 参照先                                                                                                                                   |
| ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Reactの責務・状態・IPC呼び出し  | [フロントエンドアーキテクチャ](frontend-architecture.md)                                                                                 |
| UI・CSS・共通部品・トークン     | [デザイン実装ガイド](../design/README.md) → デザイン定義の該当章・18章                                                                   |
| 権限・ゲーム起動・終了          | [共通ポリシー](sandbox-policy.md) → [macOS](macos-seatbelt.md) / [Linux](../tools/linux-validation/README.md) / [Windowsを含むCI](ci.md) |
| Microsoftログイン・トークン保存 | [認証の開発設定](microsoft-auth.md)                                                                                                      |
| ゲームへの認証・署名の仲介      | [実装状況と計画](auth-broker-plan.md) → [プローブの手順と検証記録](../tools/auth-broker-probe/README.md)                                 |
| ニュース取得・表示・RSS・記事   | [ニュースとRSS](news-feed.md)                                                                                                            |
| CI・配布物・リリース前確認      | [CIと検証範囲](ci.md)・[workflow](../.github/workflows/ci.yml)                                                                           |

日付付きの結果は、その時点・構成での記録。計画の完了条件を実装済みと読み替えず、現行コード・テスト・対象環境の結果を確認する。

## 実装の入口

| 対象                         | 最初に見る場所                                                                                                                                                |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 画面の組み立て・全体の操作   | [App.tsx](../src/App.tsx)、[useLauncher.ts](../src/app/useLauncher.ts)、[features/](../src/features/)                                                         |
| IPCの型・登録・コマンド      | [domain/](../src/domain/)、[lib.rs](../src-tauri/src/lib.rs)、[commands/](../src-tauri/src/commands/)                                                         |
| インスタンス保存・導入・起動 | [minecraft/](../src-tauri/src/minecraft/) の `model.rs`、`permissions.rs`、`installer.rs`、`launcher.rs`、`paths.rs`                                          |
| 権限変換・OSへの適用         | [minecraft/sandbox_policy.rs](../src-tauri/src/minecraft/sandbox_policy.rs) → [sandbox/](../src-tauri/src/sandbox/) → [platform/](../src-tauri/src/platform/) |
| 認証・資格情報保存・仲介     | [auth/](../src-tauri/src/auth/)、[Javaブリッジ](../src-tauri/java/)、[build.rs](../src-tauri/build.rs)                                                        |
| UIテスト・実プロセスの検証   | [tests/](../tests/)、[Rust検証CLI](../src-tauri/src/bin/)、[tools/](../tools/)                                                                                |

起動操作は `useLauncher` → `commands/minecraft.rs` → `minecraft/launcher.rs` と辿る。権限変更では保存値の `permissions.rs` と、起動時にポリシーへ変換する `sandbox_policy.rs` の両方を確認する。IPCの名前・引数・イベントを変える場合は、Rustの登録箇所だけでなく `src/` と `tests/` の呼び出し・モックも検索する。

## 起動と検証を選ぶ

コマンドはリポジトリのルートで実行する。Node / pnpmは [mise.toml](../mise.toml)、Rustは [rust-toolchain.toml](../rust-toolchain.toml) に従う。RustのビルドにもJavaブリッジ用のJDK 21が必要。OS別のビルド依存関係と初回セットアップはルートREADMEを参照する。

| 用途                               | コマンド・確認                                                                                                                            |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| ネイティブアプリを起動             | `mise run dev`                                                                                                                            |
| フロントエンドのみ起動             | `mise exec -- pnpm dev`。通常ブラウザーではゲーム操作は無効                                                                               |
| ドキュメントのみ変更               | `mise exec -- pnpm exec oxfmt --check AGENTS.md docs/README.md`（変更ファイルへ置換）、相対リンク・記載コマンドの照合、`git diff --check` |
| フロントエンド変更                 | `mise exec -- pnpm check`                                                                                                                 |
| Reactの挙動・構造変更              | 上記に加えて `mise exec -- pnpm test:ui`。初回は `mise exec -- pnpm exec playwright install chromium`                                     |
| デザイン変更                       | 上記に加えてデザイン定義18章の該当画面・操作確認。トークン変更時は先に `mise exec -- pnpm design:generate`                                |
| Rust変更                           | `mise run rust-format`、`mise run rust-lint`、`mise run rust-test`                                                                        |
| フロントエンドとRustをまとめて検査 | `mise run check`。UIテストとOS別の追加プローブは別実行                                                                                    |
| 権限・認証・OS固有の変更           | 上記に加えて対象OSのプローブ。手順と実行条件は上表の各資料と [CI](ci.md) を参照                                                           |

`pnpm check` はフロントエンドの整形・lint・トークン同期・RSSテスト・型検査・ビルドで、Rustテストを含まない。`pnpm test:ui` はTauriをモックしたReact画面とRSSの検証で、実ゲーム・認証・隔離の証明にはならない。Rustの `cfg` で分岐するコードも、ホストOSのテストだけで他OSを確認したことにはならない。

外部サービス・個人のキーチェーン等に触れるignoredテストを一括で有効にしない。必要なテストだけ、各資料の前提条件と `--ignored --exact` を使う。ネイティブの単体ビルドで最新フロントエンドを埋め込む手順や、生成RSSをRustで読む追加検証は [CI](ci.md) に従う。

## 生成物と完了確認

- `src/styles/theme.generated.css` はトークンと生成スクリプトから生成する。直接編集しない。
- `public/rss.xml`・`public/news/`・`dist/` は生成物。記事は `news/`、配信設定は `news.config.json` を変更する。
- コミット前に `git diff` とステージ済み差分を確認する。既存のpre-commit hookはステージした対象の整形とTypeScript型検査を実行するが、必要なテストの代わりにはならない。
- 検証結果には実行コマンドと対象OSを記録し、未実行・失敗・過去の結果を成功へ含めない。変更に対応する資料も更新してから、変更単位でコミットする。
