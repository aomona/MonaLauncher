# UTM Linuxデスクトップ検証環境アーカイブ

このディレクトリには、2026-09-12の検証で使用したUbuntu 24.04 ARM64 VMの作成資料を保存しています。旧`sandbox-lab` runnerは削除済みです。現在のLinux検証には[`../../linux-validation/README.md`](../../linux-validation/README.md)の本番コードを使うprobeを使用してください。

## 保存している補助ファイル

- `cloud-init.yaml` / `network-config.yaml`: 検証専用VMの初期設定
- `create.applescript`: UTM VM作成補助
- `exec.applescript`: `utmctl exec`の終了を待ち、出力と終了状態を取得する補助
- `monalauncher-sandbox-lab.apparmor`: 当時のroot所有runnerだけにuser namespace利用資格を与えた設定

AppArmor設定は履歴上の証拠として残しています。旧runnerのパスを対象にするため、現在の検証へインストールしないでください。現在のprobe用設定は`tools/linux-validation/monalauncher-probes.apparmor`です。

当時の構成・観測結果・VM UUIDは[`../results/2026-09-12-linux-utm.md`](../results/2026-09-12-linux-utm.md)を参照してください。
