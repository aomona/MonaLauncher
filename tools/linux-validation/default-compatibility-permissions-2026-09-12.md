# 既定ONの3権限（2026-09-12）

「スキンキャッシュ」「日本語入力・全画面連携」「描画キャッシュ」をPermissionsの「キャッシュとOS連携」に追加した。3つとも新規・既存インスタンスで既定ON。次回起動から適用する。

## 実装と境界

- `skinCache`: 全OSで同じ保存先・共通ポリシーへ接続。assetsDirをインスタンス専用のruntime-cache/assetsへ変更し、indexとハッシュ付きobjectはread-only、skinsのみ切り替える。hardlinkで共有できない場合だけobjectをコピーする。共有assets全体をRWにしない。
- `desktopIntegration`: macOSの実拒否ログに対応する入力候補UI・HIServices・ViewBridge・TSM・Dock全画面サービス、入力設定・ロケールに限定。基本描画と入力は固定許可のまま。Windows/Linuxは個別遮断未対応を表示し、OFFの持ち込みは明示的にONへ戻せる。
- `graphicsCache`: LinuxのMesa/NVIDIA向けは専用ディレクトリ・環境変数・read-only mount。macOSはDarwin user cache配下のnet.java.openjdk.java/com.apple.metalだけを許可する。このMetal保存先は同じユーザーのJavaアプリと共有されるため、その点をUIに表示した。Windowsは個別遮断未対応。
- 各設定はゲーム全体の書き込み設定から独立する。通信・マイク・クリップボードの既定値は変えていない。認証トークンの固定値0も今回の対象外。

## 検証

- `pnpm check` 成功。Playwrightは32件成功。3項目の既定ON、OSごとの可否、独立保存、狭幅320pxと1440px、キーボードでの到達を確認し、保存画像も目視確認した。
- macOS Rustは99 passed / 2 ignored。lib・Minecraft smoke・Linux probesのclippy（warnings禁止）は成功。全binをmacOSでclippyした初回は、既存Windows専用sandbox_probeの未使用型/importで失敗したため、適用可能なbinを対象にした。
- 実Seatbelt: スキンとインスタンス描画キャッシュのRW→RO→RW、アセットviewの書き込み拒否、保存済みスキンの読み取りを確認。ゲーム全体ROでもキャッシュ権限を独立して適用した。
- 実Seatbelt/Cプローブ: Dockのfullscreenサービスのlookupと、実際のJava Metalキャッシュ内のテスト専用ファイル作成が設定OFFで拒否・ONで成功した。テストファイルは終了時削除。入力候補サービスはプロファイルと実起動の拒否ログを確認したが、IMEでの候補操作は未検証。
- Minecraft 26.1.2の本番installer/launcher経由起動（PID 22881）: 分離したアセットviewで起動し、OpenALとSound engine初期化成功。前回のMetal・HIServices・ViewBridge・TextInputUI・Dock fullscreenの拒否はこのPIDのログでは出なかった。
- このゲームsmokeはウィンドウの存在を検出した一方、onScreen=falseだったため終了時チェックが失敗した。画面上の見た目・日本語入力・全画面操作を成功扱いしていない。ネットワークOFFの検証インスタンスを使ったため、実スキンのダウンロード成功も主張しない。
- Linux: UTM VMの起動がタイムアウトしたため、稼働中のOrbStack Ubuntu ARM64を使用した。本番のsandbox/Linux backendソースとlinux_sandbox_probeを一時的な最小crateとしてコンパイル。共通/Linuxのテスト27件と、既存の隔離境界・7フォルダー・TCP/サービス・子プロセス終了、新規キャッシュON→OFF→ONの実プローブに成功。Mesaキャッシュ環境変数も確認した。今回LinuxデスクトップやLinux版Tauri全体は検証していない。
- Windows: 実backend/common policyを参照する最小crateでx86_64-pc-windows-gnu向けcargo check成功。Windows実機のACL適用は未検証。

`pnpm tauri build --bundles app`でmacOSアプリを生成し、/Applications/MonaLauncher.appへ反映して再起動した。生成バイナリとインストール先の一致をcmpで確認した。更新後のネイティブアプリでも既存インスタンスのPermissionsを開き、3つのSwitchがONで表示されることをAXと実画面で確認した。

ローカル証拠はGit対象外の `reports/compatibility-*` に保存。権限の仕様は[共通ポリシー](../../docs/sandbox-policy.md)を参照。
