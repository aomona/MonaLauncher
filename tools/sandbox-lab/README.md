# OSサンドボックス初期検証アーカイブ

このディレクトリは、本番ランチャーへの統合前に行った独立検証の結果と環境構築資料を保存するアーカイブです。実験用Rust/Java runnerは、本番の`SandboxPolicy`と各OSバックエンドを使う検証CLIへ置き換えたため削除しました。

現在の検証手順は次を使用します。

- Linux: [`../linux-validation/README.md`](../linux-validation/README.md)
- macOS: [`../../docs/macos-seatbelt.md`](../../docs/macos-seatbelt.md)
- Windows: `src-tauri`の`sandbox_probe`と`minecraft_latest_appcontainer_smoke`

## 保存している資料

- [`results/2026-09-12-macos.md`](results/2026-09-12-macos.md): macOS Seatbelt・LWJGL初期検証
- [`results/2026-09-12-linux-orbstack.md`](results/2026-09-12-linux-orbstack.md): OrbStack上のLinux初期検証
- [`results/2026-09-12-linux-utm.md`](results/2026-09-12-linux-utm.md): UTM Linuxデスクトップ検証
- [`profiles/`](profiles/): 当時使用したSeatbeltプロファイル
- [`utm/`](utm/): UTM VMの作成・実行補助と当時のAppArmor設定

結果文書に残る`sandbox-lab`のビルド・実行コマンドは当時の記録であり、現在のcheckoutでは再実行できません。成功記録を現在の本番実装の検証結果として代用せず、上記の現行手順を実行してください。
