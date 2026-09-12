# 権限細分化の実装・検証（2026-09-12）

## 共通設定

Permissionsにゲーム全体の書き込みと用途別7項目、通信、通常音声、マイク、クリップボード、ナレーターを配置した。Windows/macOS/Linuxで同じ保存モデルと起動時ポリシーを使い、バックエンドの対応状況を画面に返す。非対応項目に効かないSwitchは置かない。

用途別の対象はワールド（saves）、screenshots、resourcepacks、shaderpacks、mods、config、logs。OFFは対象ディレクトリへの書き込み・作成・削除を拒否し、読み取りは維持する。ゲーム全体OFFが優先し、用途別の保存値は保持する。これはフォルダー境界であり、特定のMod・保存内容の意味を識別する制御ではない。ルートのoptions.txtや独自保存先は全体設定に従う。

旧JSONは追加項目の既定値で従来の状態を引き継ぐ。通信・マイク・クリップボードを自動的に新規許可しない。別OSから持ち込んだ非対応設定は、明示的な解除操作で修復できる。修復途中でも起動時には設定全体を検証し、非対応のまま起動しない。

## OSごとの制御

| 項目                         | Windows                        | macOS                    | Linux                                         |
| ---------------------------- | ------------------------------ | ------------------------ | --------------------------------------------- |
| ファイル全体・用途別書き込み | SID固有のACL/deny ACE          | Seatbelt deny file-write | 子ディレクトリのread-only bind                |
| 通信                         | 明示したネットワークcapability | TCP/UDP・DNS             | host network namespace・IPv4/IPv6・DNS/CA設定 |
| 通常音声OFF                  | 独立遮断未対応                 | CoreAudio接続許可を外す  | PulseAudio接続なし                            |
| マイクON                     | 個別変更未対応、capabilityなし | device-microphone許可    | 通常音声に含まれるサービスから分離未対応      |
| クリップボードON             | 個別変更未対応                 | pasteboardサービス接続   | 画面サービスから分離未対応                    |
| ナレーター                   | 共通ブローカー                 | 共通ブローカー           | 共通ブローカー                                |

通信ONはインターネットとLANの送受信をまとめた許可。宛先やポートのフィルターではない。Windowsのloopback例外は追加しない。マイクの実録音・macOS TCC許可は別で、今回ユーザーのマイクやクリップボードの内容は取得していない。

細分化時の既存リンク/リパースポイント/hardlinkは拒否する。Windowsでは再帰ACL更新前にも同じ検査を行う。ACLは対象AppContainer SIDの以前のdenyのみを除去して再設定し、他ユーザーのACLをリセットしない。保護フォルダーの親DELETE_CHILDと子の削除権限も拒否する。同時にファイル構造を書き換える別のホストプロセスを信頼しない場合の競合防御までは実装していない。

## 実施した確認

- macOS Rust: 95 passed / 2 ignored、clippy（lib・Linux probes、warnings禁止）成功。
- Linux Rust: 99 passed / 1 ignored。clippyと本体/probe build成功。
- フロントエンド: `pnpm check` 成功。Playwrightは31 passed。3 OSそれぞれの独立保存、全体OFFの優先、非対応項目、マイク/音声依存、別OSの設定解除、保存失敗・再試行・toastを確認した。
- 画面: Light/Dark、1440×900・1024×640・実効幅320px、200%文字/High contrast/Reduced motion、スクロールとキーボード操作。保存した実画面画像も確認した。
- macOSの実Seatbelt: 制限フォルダー内の新規作成・既存更新・フォルダー移動を拒否し、別領域の書き込みと保護データの読み取りを維持した。TCPの許可/拒否、CoreAudioとpasteboardのMach lookupの許可/拒否、device-microphoneのポリシー照会を確認した。マイク収録や実クリップボード読取りはしていない。
- macOS DNS: 本番のcore/networkプロファイルを使う別Cプローブでexample.orgの名前解決を確認。通信OFFは失敗（EAI_NONAME）、ONは成功した。最初の簡略プロファイルは起動に失敗したため、その結果を制御成功に数えていない。
- Linux UTM: productionの `linux_sandbox_probe --wayland` で7フォルダーの読み取り・書き込み拒否・移動拒否、その他の書き込み、TCP/PulseAudioのOFF/ON、AF_NETLINK拒否、従来のファイル/ネットワーク境界・子終了が成功した。
- Linux実ゲーム: Minecraft 26.2をnative Waylandで音声OFF/ONそれぞれ起動。OFFではOpenALデバイス接続が失敗し、JavaのPulseAudio streamなし。ONではSound engineとstreamが復帰。両方でgraphics初期化とナレーターブローカーの再生開始・完了を確認した。音声OFF時のSoundSystemエラーは要求した遮断の結果。自動プローブはWayland window visibilityを判定しない。
- Linux実画面: 更新したランチャーのPermissionsからscreenshotsWriteをOFFに保存し、Playで起動したMinecraftのタイトル画面を目視確認。F2保存はRead-only file systemで拒否され、通常音声とlogsへの保存は動作した。Quit Game後に画面からONへ戻して再起動するとF2保存が成功した。検証用インスタンスの通常音声・スクリーンショット保存はONへ復帰済み。Linux固有のマイク・クリップボードの未対応説明と縦スクロールも実画面で確認した。
- Windows: 実際の変更対象のWindows backend/common policyソースを一時的な最小crateから参照し、x86_64-pc-windows-gnu向け `cargo check` 成功。Tauri全体のWindowsビルドやWindows実機での適用確認ではない。
- Windows実機用の既存 `sandbox_probe --acl` を拡張し、7領域の許可→拒否→再許可→全体読み取り専用、既存更新・新規作成・renameを検査するようにした。このホストでは実行していない。CIのWindowsジョブで実行する項目であり、成功済みとは扱わない。

## 証拠と参照

ローカルログ・画像はGit対象外の `reports/granular-*`。通常の境界プローブは[README](README.md)、権限の意味と例外は[共通ポリシー](../../docs/sandbox-policy.md)を参照。

- [Microsoft AppContainer isolation](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation)
- [Microsoft icacls](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/icacls)
- [ChromiumのmacOS audio profile](https://github.com/chromium/chromium/blob/main/sandbox/policy/mac/audio.sb)
- [ChromiumのmacOS network profile](https://github.com/chromium/chromium/blob/main/sandbox/policy/mac/network.sb)
