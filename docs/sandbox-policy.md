# 共通サンドボックスポリシー

Minecraftの権限指定は `src-tauri/src/sandbox/` にまとめる。`minecraft/sandbox_policy.rs` が検証済みインスタンスからリソースの実パスを解決し、Windows・macOS・Linuxの起動処理へ同じ `SandboxPolicy` を渡す。InstanceのPermissionsタブからゲーム全体/用途別の書き込み、通信、対応OSでの通常音声・マイク・クリップボード、ナレーターを設定できる。任意のパスやOS固有の追加許可を受け取るAPIは設けていない。対象はゲームプロセス全体であり、個々のModを識別した権限制御ではない。

## 指定と適用

| 共通リソース                                     | Minecraft既定の指定                 |
| ------------------------------------------------ | ----------------------------------- |
| 管理下Java、ライブラリ、アセット、選択バージョン | 読み取り専用                        |
| 当該インスタンスのgame                           | 読み書き                            |
| 起動用ファイル                                   | 読み取り専用                        |
| 起動ごとのtmp                                    | 読み書き                            |
| ゲーム自身の通信                                 | ネットワーク権限を付与しない        |
| 画面・入力・通常音声                             | Minecraftデスクトップ互換設定を許可 |
| ナレーター                                       | ランチャーの共通ブローカーを許可    |

実パスは正規化してから確認し、書き込み先とコード領域の重なり、gameとlaunchの重なり、所有ディレクトリ外への参照を拒否する。読み取り専用は内容の変更を許可しない指定であり、ネイティブコードのロードを禁止する指定ではない。Windowsでは従来と同じRXへ変換する。

- **Seatbelt**: OSの互換ルールと、共通ポリシーから生成したファイル許可を合成する。実パスは `sandbox-exec -D` の値として渡し、ルール文字列へ埋め込まない。
- **AppContainer**: 共通ポリシーをファイルACL・継承しない通過用ACLへ変換する。親の読み取り許可を先に適用し、gameや起動領域の変更許可と整合性レベルを適用する。プロセス作成前にもポリシーを検査し、通信OFF時はCapabilityをゼロにし、ON時は明示したネットワークCapabilityだけを追加する。
- **ナレーター**: 同じポリシーからJava側の有効状態とホストのブローカー起動を制御する。無効時はJavaが読み上げ要求を出さず、ホストにもブローカーを作らない。

`with_file_access` はgame/tmpを読み取り専用へ縮小できる。共有コード等への書き込み追加、画面と入力の個別禁止、各OSで未対応の音声・マイク・クリップボード設定は拒否する。要求を無視した起動や、非サンドボックス起動へのフォールバックはしない。

## 明示したOS差分

Windowsの既存の動作を保つため、Minecraft既定ポリシーは `allow_windows_compatibility = true` を明示する。変換結果の `exceptions` には次の追加許可が入る。falseならAppContainer起動を拒否する。

| Windowsの追加許可                      | 現在の理由・範囲                                               |
| -------------------------------------- | -------------------------------------------------------------- |
| 起動用ディレクトリへの書き込み         | 既存のAppContainer起動領域の互換動作を維持                     |
| 当該インスタンスのメタデータの読み取り | 既存のinstanceルートRX継承を維持。game以外への変更は許可しない |
| versions全体の読み取り                 | 既存の共有バージョン領域へのRXを維持                           |
| OSが用意するAppContainer専用ストレージ | Windowsの既知フォルダー・一時領域の仮想化を維持                |

Windowsのtmpは変更可能なlaunchの子なので、tmpだけを読み取り専用にする要求は拒否する。親からの書き込み継承を無視して「読み取り専用」と報告しない。

共通化はOSの保証を同一にするものではない。Seatbeltにはシステム領域の読み取り・広いメタデータ参照・列挙したMach/IOKitサービスがある。WindowsにはOSがAppContainerへ公開する資源や既知フォルダーがある。子の終了はWindowsがJob Object、macOSがプロセスグループで、ランチャー異常終了時やグループ離脱時の保証も異なる。これらを同等の隔離と扱わない。

Linuxは `Backend::Bubblewrap` へ変換する。`platform/linux` が新しいuser/PID/network/IPC/mount namespace、ファイルのbind mount、seccompを構成する。`--unshare-user` と `--disable-userns` を明示し、追加のnamespace作成も拒否する。seccompはネイティブABIのみを許可し、通信OFF時はAF_UNIX以外のsocketを拒否する。ON時はホストnetwork namespaceを共有してAF_INET/AF_INET6とDNS/CA設定の読み取りを追加し、AF_NETLINK等は拒否を維持する。ptrace、mount、BPF、keyring等も常に拒否する。clone3はENOSYSを返し、JVMの通常のthread作成はcloneへフォールバックできる。フィルターのFD以外はゲームへ追加継承せず、stdinは閉じる。起動失敗を通常起動へフォールバックしない。

Linuxの `allow_linux_desktop_compatibility = true` は以下を明示的に受け入れる。falseならLinuxデスクトップ起動を拒否する。

- システム実行環境（`/usr`・ライブラリ・列挙したフォント/loader設定）の読み取り。ホーム全体や `/etc` 全体は公開しない。
- Waylandを選んだ場合は単一のcompositorソケットだけを公開し、DISPLAY・Xauthority・X11のパス/abstractソケットは公開しない。X11/XWaylandを選んだ場合は従来のソケットと認証ファイルを公開し、他のX11クライアントの観測・操作を防ぐものではない。
- Waylandでは接続先compositorが提供するプロトコル全体への接続を許す。クリップボード等の権限を分離するプロトコルフィルターやFlatpak型security-contextは未実装であり、ウィンドウ描画だけの許可や全compositor共通のクライアント間隔離を保証しない。
- 選択したローカルPulseAudio互換ソケット。録音・音声サーバー操作も含まれ、出力専用の権限ではない。外部接続を含むホストサービス経由の間接操作も、直接のネットワークsyscall拒否と区別する。
- 存在するDRM render node。Waylandのlibdrm初期化用に、そのGPUの識別属性（uevent、vendor/device/subsystem IDs、revision、DRM nodeのdev）を個別に読み取り専用bindし、必要なディレクトリ/リンクを再構成する。GPUのprimary nodeの識別情報も含むが、デバイス自体・PCI config/resource・コネクター・ホストのsysfs全体は公開しない。`/dev/input`・DRM primary node・`/dev/snd`・D-Bus・ホストの `/run/user` 全体は公開しない。

`WAYLAND_DISPLAY` またはWaylandの `XDG_SESSION_TYPE` がある場合はWaylandを優先する。名前付きソケットを検証できなければ起動エラーとし、X11へ切り替えない。継承FDを指定する `WAYLAND_SOCKET` は未対応として拒否する。Minecraft 26.2はGLXが明示的にX11を選ぶため、このバージョンのWayland起動だけに検証済みの `MC_DEBUG_ENABLED` / `MC_DEBUG_PREFER_WAYLAND` JVMプロパティを追加する。他バージョンやModのWayland互換性は別途検証が必要。

ナレーターは共通の認証済みstdoutプロトコルを、ホストのeSpeak NGへ渡す。テキストは標準入力で渡し、シェルのコマンドやファイル名に使わない。無効時はブリッジから要求せず、ブローカーも起動しない。一般音声の生成能力やMod独自の音声合成まで禁止する設定ではない。

Linuxのプロセス終了ではbubblewrapの監視プロセスを停止する。PID namespace内でセッションを分離した子も終了することを実プローブで確認する。カーネル脆弱性への耐性や、CPU/メモリ/ディスクの使用量上限を保証するものではない。

UbuntuのAppArmor user namespace制限を持つ環境では、root所有のパッケージ実行ファイルへのプロファイル登録が必要。`packaging/linux/monalauncher.apparmor` を参照。システム全体のAppArmorやsysctlは無効にしない。依存関係・再現手順・今回の観測は [Linux検証手順](../tools/linux-validation/README.md) を参照。

## 検証

Linux本体への統合後の確認は [2026-09-12 Linux統合検証](../tools/linux-validation/results-2026-09-12.md) に記録した。以下は先行する共通化・macOS検証の記録。

- 共通テスト: 両OSのファイル割当、Windowsの明示的追加許可、未対応要求の拒否、読み取り専用への縮小、パスの重なり・リンクの拒否。
- macOS実Seatbelt: 共通ポリシーからの許可・拒否、リンク経由アクセス、子プロセスの制限、gameを読み取り専用にした場合の書き込み拒否。
- Javaブリッジ: ビルド時にナレーター許可・不許可の両方を検証。
- Windows CIの既存ACLプローブは共通ポリシーを経由するよう更新。AppContainer/ACLの実適用はWindowsホストで確認が必要で、このmacOS上のテストだけでは保証しない。

コマンド:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked
MONALAUNCHER_EXPECT_NARRATOR=1 cargo run --manifest-path src-tauri/Cargo.toml --locked --bin minecraft_smoke -- "$HOME/Library/Application Support/me.aomona.monalauncher/minecraft" 25 26.2
```

Windowsでは既存CIの `cargo run --locked --bin sandbox_probe -- --acl` と `--appcontainer` を実行する。

2026-09-12の確認結果: Rustテスト81件成功（外部認証関連2件は通常実行で除外）、対象ライブラリとMinecraft検証CLIのClippy成功、Javaブリッジの許可・不許可チェック成功、macOSアプリのビルド成功。Seatbelt既定値の操作・条件を展開して変更前と比較し、同じ許可集合であることも確認した。

26.2の実ゲームでは生存・音声エンジン初期化・ナレーターの開始と完了・停止を確認した。ただしユーザーが別デスクトップ／全画面アプリを使用中だったため、終了時の画面表示判定はfalseで、検証コマンド全体はPASSとしていない。Windowsのコードは共通ポリシーへ接続済みだが、この変更後のWindows CI・実機実行は未確認。

## インスタンスごとの保存設定

`instance.json` の `permissions` は `{ "gameWrite": true, "narrator": true }`。フィールドがない既存ファイルだけ従来の既定値を補う。設定オブジェクト内の欠落・不明フィールド・型違いは拒否し、不明な権限要求を無視しない。

`update_minecraft_permissions` はインスタンス操作を予約し、ゲーム実行中・他操作中・未対応OS・旧形式のインスタンスを拒否する。既存の検証済みmanifestを読み、permissionsだけを変更して原子的に保存する。改名・修復はこの設定を維持する。保存は次回起動向けで、実行中のプロセスの権限を変更しない。

`policy_for_instance` は保存した設定を共通ポリシーへ適用する。ゲーム領域のReadOnly指定とナレーターブローカーの無効化はWindows/macOS/Linuxの変換に渡す。UIはOSの対応状況をバックエンドから取得し、対応状況が不明な間も変更できない。書き込み無効はログや設定の保存も止めるため、バージョンによってゲームが起動しない可能性がある。

追加検証: 既存設定の移行、厳格な入力検査、実ファイルへの保存と再読込、改名後の保持、別インスタンスの分離、保存値から各OSのポリシーへの変換を確認。Windows ACL実適用は引き続きWindowsホストでの確認が必要。

## 細分化した権限

共通データの既定値は、従来のゲーム書き込み・通常音声・ナレーターを維持し、network/microphone/clipboardはfalse。追加したフォルダー設定の既定値trueは、既存のゲーム全体設定を広げない。ゲーム全体OFFが最優先になる。

| 設定                           | Windows                                            | macOS                                | Linux                                          |
| ------------------------------ | -------------------------------------------------- | ------------------------------------ | ---------------------------------------------- |
| 全体・用途別のファイル書き込み | AppContainer SIDのACL/deny ACE                     | Seatbeltの書き込み拒否               | 子ディレクトリのread-only bind                 |
| 通信ON                         | Internet client/server・private network capability | TCP/UDP・DNSサービスの許可           | host network namespace + IPv4/IPv6 syscall許可 |
| 通常音声OFF                    | 独立遮断は未対応                                   | CoreAudioのMach/共有メモリ許可を外す | PulseAudioソケットを公開しない                 |
| マイクON                       | 個別変更未対応・capabilityなし                     | device-microphoneを許可。TCCは別途   | 音声サービスからの独立制御未対応               |
| クリップボードON               | 独立制御未対応                                     | pasteboardサービスへの接続を許可     | 画面接続からの独立制御未対応                   |
| ナレーター                     | 共通の認証済みブローカー設定                       | 同左                                 | 同左                                           |

用途別の対象は `saves/screenshots/resourcepacks/shaderpacks/mods/config/logs` だけ。ゲーム/Modを用途ごとに識別するものではなく、OFFでもそのデータの読み取りや別フォルダーへのコピーは可能。`options.txt` や独自の保存先は全体の書き込み設定に従う。読み取り専用にするとゲームやModの起動・保存が失敗する場合がある。

細分化時はゲームツリーのシンボリックリンク・reparse point・既存hardlinkを拒否する。Windowsの再帰ACL更新でも同じ検査を行い、対象SIDの以前のdenyだけを消して再適用する。保護ディレクトリには書き込み/削除/ACL変更拒否、親には非継承のDELETE_CHILD拒否を設定する。ランチャー操作の直前に別のホストプロセスが同時にファイル構造を書き換える状況まで保証するものではない。

通信ONはインターネットとLANへの送受信をまとめた許可で、宛先・Mod・ポートのフィルターではない。Windowsのloopback制限は自動解除しない。macOSはDNS用のmDNSResponder接続、LinuxはDNS/CA設定を公開する。通信OFFへ戻した次の起動ではこれらの追加許可を付けない。

macOSのマイクは通常音声と同じCoreAudioサービスを必要とするため、音声OFF/マイクONは拒否する。マイクのOSプライバシー許可や実録音は別。Linuxは通常音声ONで録音を含むPulseAudioサービス全体へ接続できる。クリップボードについてもWindows/Linuxで偽の拒否状態を表示せず、未対応であることを表示する。

別OSから持ち込まれた非対応の設定を黙って広げない。保存APIは新しい非対応要求を拒否し、既存の非対応設定の保持・縮小を認める。起動時はポリシー全体を再検証し、非対応の設定が残っていればエラーにする。実施結果は[権限細分化の検証記録](../tools/linux-validation/granular-permissions-2026-09-12.md)を参照。
