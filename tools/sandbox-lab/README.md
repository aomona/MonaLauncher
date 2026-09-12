# OSサンドボックス初期検証

本番ランチャーから独立した実験用ツール。共通APIの確定前に、同じ操作が各OSのサンドボックスでどう制限されるかを確かめる。ゲーム起動処理や既存AppContainerの設定は変更しない。

採用方針は Windows = AppContainer、macOS = Seatbelt、Linux = bubblewrap + seccomp。現時点の実験用Linuxバックエンドは **namespaceとmountだけ**で、seccompはまだ実装していない。GUIはX11とDRM render nodeを対象に初期検証する。この段階の合格を製品のサンドボックス完成とは扱わない。

## 再実行

リポジトリのルートで実行する。Rust 1.97.1、JDK 21、macOSの`sandbox-exec`、Linuxでは`bwrap`が必要。依存するJARは信頼できる検証用LWJGLだけで、ユーザーのMODはこのツールで実行しない。

### macOS: Rust・Java・子プロセス

```sh
probe_java_home="$(/usr/libexec/java_home -v 21)"
cargo run --manifest-path tools/sandbox-lab/Cargo.toml --locked -- \
  --java-home "$probe_java_home" \
  --report tools/sandbox-lab/reports/macos-headless.json
```

### macOS: LWJGLのウィンドウとGPU描画を追加

```sh
probe_lwjgl_dir="$(node tools/sandbox-lab/prepare-lwjgl.mjs)"
cargo run --manifest-path tools/sandbox-lab/Cargo.toml --locked -- \
  --java-home "$probe_java_home" \
  --lwjgl "$probe_lwjgl_dir" \
  --report tools/sandbox-lab/reports/macos-lwjgl.json
```

Node 24を使用する。ダウンローダーはLWJGL 3.3.3のcore・GLFW・OpenGLと対象CPUのnative JARをMaven Centralから取得し、`lwjgl-artifacts.json`のSHA-256を確認する。マニフェスト作成時にはMaven側のSHA-1とも照合済み。キャッシュは`.cache/`に保存する。

1.5秒程度のウィンドウが隔離なし・隔離ありの各1回表示される。OpenGL 3.2のコンテキストで背景を塗り、ピクセルが期待するRGBA値かを検証する。GUI権限を追加したプロファイルでも、RustとJavaの隔離テストを再実行する。標準出力・標準エラーは実際のパイプで取得する。

### Linux: 共通のヘッドレス検証

```sh
cargo run --manifest-path tools/sandbox-lab/Cargo.toml --locked -- \
  --java-home /absolute/path/to/jdk-21 \
  --report tools/sandbox-lab/reports/linux-headless.json
```

`bwrap`と非特権user namespaceを利用できるホストが必要。利用できなければ失敗として扱い、ホストのsysctlやAppArmor設定を自動変更したり、隔離なしで代替したりしない。[OrbStack上のUbuntu ARM64での実行結果と再実行手順](results/2026-09-12-linux-orbstack.md)を保存済み。

Debian/UbuntuのOpenJDKでは`conf/security/java.security`や`conf/net.properties`がJDK外へのsymlinkになっている。Linuxバックエンドはこの2ファイルの実体を解決し、JDK外の場合にそのファイルだけを読み取り専用で公開する。`/etc`全体やJava設定ディレクトリ全体は公開しない。

### Linux: UTMデスクトップでLWJGLを検証

[UTM環境の作成・実行手順](utm/README.md)を参照。Linux arm64/x64用native JARもダウンローダーの対象。X11デスクトップの`DISPLAY`と`XAUTHORITY`を指定し、macOSと同じ`--lwjgl`オプションで実行する。GUIプロファイルでもファイル・ネットワーク・子プロセスの検証を繰り返す。

X11は選択したローカルUnix socket、描画はDRM render nodeを公開する。一般ネットワークの共有は加えない。X11クライアント間のアクセス、Wayland、音声、入力デバイスの検証は今後の作業。

### Windows: 既存の検証を維持

```sh
cargo run --manifest-path src-tauri/Cargo.toml --locked --bin sandbox_probe -- --appcontainer
cargo run --manifest-path src-tauri/Cargo.toml --locked --bin sandbox_probe -- --acl
```

Windowsの共通fixtureへの接続は次段階。既存のCI・起動実装をそのまま利用する。

## 判定方法

信頼した親プロセスが、使い捨てのファイル・TCP listener・UDP echo・Unix socketを用意する。実在する秘密ファイルやインターネットの接続先は使わない。

まず隔離なしで **全操作が成功する** ことを確認し、同じfixtureに対して隔離ありの結果を比較する。失敗時は非ゼロ終了する。対象プロセスが起動しなかった場合、結果が欠ける場合、重複する場合、対照実験が失敗した場合を合格にしない。

| 操作                                                                | 隔離なし | 隔離あり |
| ------------------------------------------------------------------- | -------- | -------- |
| 共有assetの読み取り                                                 | 成功     | 成功     |
| インスタンス設定の読み取り                                          | 成功     | 成功     |
| ゲーム領域へのファイル書き込み                                      | 成功     | 成功     |
| ホストの秘密ファイル相当の読み取り・書き込み                        | 成功     | 拒否     |
| 他インスタンス設定の読み取り                                        | 成功     | 拒否     |
| 共有asset・インスタンス設定の書き込み用open                         | 成功     | 拒否     |
| ゲーム領域からホスト領域へのsymlinkを通した読み取り・書き込み用open | 成功     | 拒否     |
| ホストのIPv4 loopback TCPへの接続                                   | 成功     | 拒否     |
| ホストのIPv4 loopback UDPとのecho往復                               | 成功     | 拒否     |
| ホスト領域に置いたUnix socketへの接続                               | 成功     | 拒否     |

合計13操作を、Rust実行ファイルとJavaそれぞれで親・子の2世代に対して行う。子にはサンドボックスコマンドを追加しないので、制約の継承を確認できる。Java側の子も別のJVMとして起動する。

`passed`はこの実行で要求した検証の合否のみを示す。JSONの`completed`、実行開始時刻、`not_tested`、各実行のexit status、stdout、stderrを併せて読む。セットアップ前に既存の成功レポートを無効化するため、Javaが見つからない場合や実行中断時も過去の成功を残さない。fixtureは実行後に削除し、レポートとダウンロードキャッシュはGit管理対象外。

## 実験用ポリシーの境界

- macOSは`deny default`から始め、canonical pathをSBPLパラメーターとして渡す。ホーム全体や共有`/tmp`へのファイル内容の読み取り・書き込みは許可しない。
- 起動互換性のため、ファイルのメタデータとsysctlの読み取りは広く許可している。**ファイルの存在・メタデータを隠す試験ではない。** Javaの配布ディレクトリ、OSライブラリ、fixtureの実行領域も読み取り可能。
- GUI用のMach service・IOKit許可は別ファイルに記録する。Mach service全体の一括許可、Keychain、Apple Events、汎用ネットワーク許可は加えていない。ただし許可したサービスによる代理アクセスは未検証。
- Linuxは空のmount namespaceに必要な領域をbind mountし、ネットワーク・PIDなどのnamespaceを分離する。`/usr`などのシステム領域は実証用に広く読み取り公開している。
- 40秒のプロセス群タイムアウトは、信頼した試験コードのハング対策。悪意あるプロセスの逃走防止やランチャー異常終了時の子孫回収を証明しない。
- GUIの作成とframebufferの読み戻しは、キーボード・マウス・音声・Minecraftのプレイ動作の検証には代えない。

## 初回結果と次の作業

[macOS実機結果](results/2026-09-12-macos.md)と[OrbStack Linuxゲストの結果](results/2026-09-12-linux-orbstack.md)を参照。

1. 標準カーネル・AppArmorなどの制約が有効なLinux環境でも同じfixtureを実行する。OrbStack上での合格を全ディストリビューションの保証にはしない。
2. Flatpakの実装を参照し、Linuxのseccomp、Wayland/X11、DRI、音声、D-Busの必要な制約を実装・検証する。
3. macOSの実機検証を入力・音声・Minecraftへ広げる。現在残っているGUI関連の拒否を、必要な操作と対応付ける。
4. Windowsの既存AppContainerへ共通fixtureを接続する。
5. 実測したOS差を踏まえて、本番用の`LaunchSpec`・ポリシー・プロセス管理APIを確定する。

## 参考実装

実装方法・ポリシー分離を参考にした。リポジトリ全体を依存として取り込んではいない。

- [arapuca: Sandbox trait](https://github.com/LeGambiArt/arapuca/blob/a9efe351e6db2d129f5393442feed51281361c56/src/platform/mod.rs)、[Seatbeltプロファイル](https://github.com/LeGambiArt/arapuca/blob/a9efe351e6db2d129f5393442feed51281361c56/src/platform/darwin/darwin_profile.rs)（Apache-2.0）: 共通実行契約、パス解決、dyldが必要とするルートの読み取り。
- [Anthropic sandbox-runtime: macOS](https://github.com/anthropics/sandbox-runtime/blob/c392e6cf9f8df957c66d9ab1461e2cfa99b1ab5d/src/sandbox/macos-sandbox-utils.ts)（Apache-2.0）: deny-defaultとサービス別の許可。
- [MXC: Seatbelt](https://github.com/microsoft/mxc/blob/567570084f1ebaca539b0a3186aeb68bca77788a/src/backends/seatbelt/common/src/profile_builder.rs)（MIT）: GUI用のMach/IOKit許可。今回の許可一覧は実機の拒否ログで絞り込んだもの。
- [Convira: capability tests](https://github.com/Convira/convira-sandbox/blob/9013c323725155f1e2e3e95c597715c172dff0c1/packages/sandbox-runtime/__tests__/capability-report.test.ts)（Apache-2.0）: 観測していない能力を成功扱いしない方針。
- [bubblewrap](https://github.com/containers/bubblewrap#sandbox-security)、[Flatpakの権限設計](https://docs.flatpak.org/en/latest/sandbox-permissions.html)、[Flatpakの起動実装](https://github.com/flatpak/flatpak/blob/main/common/flatpak-run.c): Linuxの次段階の参照先。

## 開発時チェック

```sh
cargo fmt --manifest-path tools/sandbox-lab/Cargo.toml --check
cargo clippy --manifest-path tools/sandbox-lab/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path tools/sandbox-lab/Cargo.toml --locked
```

このcrateはTauriのcrateと独立しているため、`src-tauri/Cargo.toml`に対するチェックだけでは検証されない。
