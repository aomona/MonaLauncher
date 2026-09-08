# プロジェクト概要

MonaLauncher は、Windows の AppContainer 内で Minecraft を実行する開発中のランチャーです。Tauri 2 / Rust をバックエンド、React / TypeScript / Vite をフロントエンドに使用しています。ゲームと Mod のファイルアクセスを制限し、ゲームプロセスにネットワーク権限を付与しないことで、ホスト環境からの隔離を図ります。

# 作業方針

- 変更ごとに、内容に応じた確認を行ってから Git コミットを作成してください。
- コミットは意味のある変更単位に分け、変更内容が分かるメッセージを付けてください。
- 自分の作業に関係のない既存の変更はコミットに含めないでください。

# デザイン実装

- UI・CSS・コンポーネントを変更する前に [design/README.md](design/README.md) を読み、実装方法と検証手順に従ってください。
- 挙動・画面構成は [デザイン定義 v1.1](design/MonaLauncher_Design_Definition_v1.1.md) の該当章、数値は [デザイントークン v1.1](design/MonaLauncher_Design_Tokens_v1.1.json) を参照してください。既存画面の配色・装飾・寸法を新デザインの根拠にしないでください。
- Tailwind CSS v4 の設定は `src/styles/theme.generated.css` です。生成物を手編集せず、トークンまたは `scripts/generate-design-theme.mjs` を変更して `pnpm design:generate` を実行してください。
- 添付原本は参照資料として保持します。仕様変更を依頼された場合に限り更新し、ガイド・生成設定との整合性を確認してください。
- デザイン変更後は `pnpm check` と、変更範囲に対応するデザイン定義18章の画面・操作検証を行ってください。設定の検証だけで画面全体を実装済み・アクセシビリティ適合済みと扱わないでください。
