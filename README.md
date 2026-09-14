# MonaLauncher

MonaLauncherは、Minecraft Java EditionをOSのサンドボックス内で実行する、開発中のランチャーです。ゲームとModによるファイルアクセスや通信を、インスタンスごとの権限設定で制限します。

バックエンドはTauri 2 / Rust、フロントエンドはReact / TypeScript / Viteを使用しています。

## 主な機能

- **インスタンス管理**: Vanilla・Fabricのインスタンスを作成し、Minecraftを起動できます。
- **Mod管理**: Fabricインスタンス向けに、ModrinthのModを検索・導入・管理できます。
- **権限設定**: インスタンスのPermissionsから、ゲームデータへの書き込み、通信、対応OSでの音声・マイク・クリップボードなどを設定できます。
- **Microsoft認証**: ブラウザーでのサインインと、対応構成でのゲーム認証の仲介を実装しています。
- **ニュース**: HomeとNewsで記事を閲覧できます。MonaLauncherの記事はアプリ内で開き、取得済みの本文はオフラインでも読めます。

## 対応OSと制限

| OS      | サンドボックス       | 制限・検証資料                                                                                                             |
| ------- | -------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Windows | AppContainer         | [共通ポリシー](docs/sandbox-policy.md)・[CIの確認範囲](docs/ci.md)。通信ON/OFFの実接続は通常CIの対象外です。               |
| macOS   | Seatbelt             | [起動手順・実機検証記録](docs/macos-seatbelt.md)。実験的な対応で、Windowsと同じプロセス終了保証はありません。              |
| Linux   | bubblewrap / seccomp | [依存関係・検証手順](tools/linux-validation/README.md)。トークン保存は未対応で、デスクトップ環境によって制限が異なります。 |

権限制御の対象はゲームプロセス全体です。同じゲーム内のModを個別に識別して隔離するものではありません。

- ゲーム自身の通信は既定でOFFです。PermissionsからONにできますが、アカウント認証の仲介は別の設定です。通信を許可しただけで、マルチプレイやRealmsが動作するとは限りません。
- 管理下のJava・ライブラリ・アセットは読み取り専用とし、ゲームデータなどの指定領域に書き込みを限定します。OSの互換動作に必要な追加許可や、音声・クリップボードの制限にはOS差があります。
- 未対応の権限要求やサンドボックスの起動失敗はエラーにします。非サンドボックス起動へのフォールバックは行いません。

各OSの許可範囲と終了処理の違いは[共通サンドボックスポリシー](docs/sandbox-policy.md)を参照してください。CIや特定環境での検証は、すべてのMinecraft・Mod・OS構成での動作や安全性を保証するものではありません。

## 開発環境で起動する

事前に次を用意してください。

- mise（Node.js・pnpmのバージョンは[mise.toml](mise.toml)で固定）。
- rustup（Rustのバージョンと追加コンポーネントは[rust-toolchain.toml](rust-toolchain.toml)で指定）。
- JDK 21。`java`・`javac`・`jar`をPATHから実行できる状態にします。ゲーム用とは別に、同梱するJavaブリッジのビルドに必要です。
- [Tauri 2のOS別ビルド依存関係](https://v2.tauri.app/start/prerequisites/)。Linuxでゲームを起動する場合は、追加の[依存関係とAppArmor設定](tools/linux-validation/README.md)も確認してください。

このリポジトリを取得したディレクトリで実行します。

```sh
mise install
mise exec -- pnpm install --frozen-lockfile
mise run dev
```

`mise run dev`はTauriアプリを起動します。`pnpm dev`はフロントエンドの開発サーバーだけを起動します。配布物のビルドには`mise run build`を使います。

Microsoft認証には既定の公開クライアントIDが組み込まれているため、通常は環境変数の設定や独自のアプリ登録は不要です。アカウント設定からサインインを開始し、表示されたコードをブラウザーで入力します。別のクライアントIDを使う場合や認証仲介の制限は[Microsoft認証の開発設定](docs/microsoft-auth.md)を参照してください。

## 検証

```sh
mise run check
mise exec -- pnpm exec playwright install chromium
mise exec -- pnpm test:ui
```

`mise run check`はフロントエンドとRustの整形・静的検査・テスト・フロントエンドビルドを実行します。`pnpm test:ui`はTauriをモックした画面操作の検証です。OS固有のプローブ、配布物と実ゲームの確認は[CIとリリース前の確認](docs/ci.md)を参照してください。

## 詳細資料

- [共通サンドボックスポリシー](docs/sandbox-policy.md)
- [Microsoft認証の開発設定](docs/microsoft-auth.md)
- [ニュースとRSSの配信手順](docs/news-feed.md)
- [フロントエンドアーキテクチャ](docs/frontend-architecture.md)
- [デザインの実装・検証ガイド](design/README.md)
