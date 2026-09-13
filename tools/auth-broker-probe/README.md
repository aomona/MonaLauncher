# Fabricによるトークン読み出し検証

このModは実際のMinecraftのUser/Sessionからトークンを読み出し、親側で生成したランダムな検証用トークンのSHA-256と比較する。トークン本体・読み出した値・例外本文を結果へ記録せず、検出した面と実行できた検査だけをJSONへ書く。Microsoftログインや実アカウントの資格情報を使わない正の対照実験で、仲介実装後も同じ検出器を使う。

ModはFabric Loaderのclient entrypointで実行する。Fabric APIやMixinを追加せず、公式名（26.2）またはFabric MappingResolver（1.21.8）で実ゲームのsingletonを取得する。[Fabric Loader](https://docs.fabricmc.net/develop/loader/)

## ビルドと実行

Java 21以上、プロジェクトのRust、インストール済みFabric Loaderが必要。

```sh
python3 tools/auth-broker-probe/build.py --loader /absolute/path/to/fabric-loader-0.19.5.jar
cargo run --manifest-path src-tauri/Cargo.toml --locked --bin auth_broker_probe -- \
  /absolute/path/to/minecraft \
  tools/auth-broker-probe/build/mona-token-read-probe.jar \
  tools/auth-broker-probe/reports/brokered-26.2-macos.json 26.2
```

末尾のversionは1.21.8または26.2。専用の `auth-probe-brokered-*` インスタンスを作成し、既存のユーザーインスタンスへModを入れない。ライブラリ・Javaは通常の検証済みインストーラーを共有する。通信OFF、ナレーターOFFで、最大120秒の実プロセス検証後に所有するゲームを停止する。

現行ハーネスは `tokenDetected=false`、`agentAdapterPresent=true`、`chatAdapterPresent=true`、`brokerHandshakeCompleted=true`、`liveJavaHeapDumpScanned=true` を合格条件とする。ゲーム起動中の実authlib呼び出しでRust仲介とhelloを交換し、通信OFFのため外部認証要求は拒否される。これはオンライン参加成功の試験ではない。

正の対照はコミット `fd02af8` の同じハーネスで再現できる（別worktreeで実行する）。そこでは直接渡したランダムな検証用トークンをUser/Sessionから実際に検出した。現行の製品コードには実トークンを直接渡す検証用フォールバックを残さない。

## 検査範囲

- 実User/Sessionの文字列フィールド。
- Minecraft/authlibの到達可能なオブジェクトフィールド（深さ5・2万オブジェクトまで）。
- JVM起動オプション、OSが公開する自プロセスコマンド、環境変数、システムプロパティ。取得できないプロセス情報は `unavailableSurfaces` に分ける。
- 専用ゲームディレクトリの通常ファイル（深さ4・512件・1件256 KiB・合計8 MiBまで）。リンクは追跡しない。

- HotSpotの生存中オブジェクトのヒープダンプを作り、全バイトを読み、検証用hexトークンをLatin-1/UTF-8・UTF-16 BE/LE表現で探す。ダンプは検査後に削除し、ハーネスも異常終了後の残存ダンプを削除する。ファイルサイズはレポートに残す。

生存していないオブジェクトを含むヒープ全体、変換・分割された値、ネイティブメモリ、ランチャーのメモリは未検査。したがって `wholeHeapScanned` はfalseのままとし、生存中ヒープの検査だけを `liveJavaHeapDumpScanned` で示す。署名鍵・オンラインサーバー接続の検証も追加工程。

2026-09-13のmacOS Seatbelt実行では、1.21.8と26.2の双方でFabric entrypointが実行され、実User/Sessionから検証用トークンを検出。検査結果は `baseline-macos.json` に保存。

仲介後のmacOS実ゲーム結果は `brokered-macos.json` に保存。この旧記録の時点では秘密鍵取得とチャット署名は未実装だった。最新の署名仲介の検証範囲は後述する。Windows IPCは未実装。Linuxの26.2経路は後述の独立したゲスト内で検証した。

## 生存中ヒープを含む比較

`heap-comparison-2026-09-13.json` は同じMod JAR（SHA-256を記録）を使った、macOSの1.21.8・26.2とLinuxの26.2の計6実行。直接受け渡しの3実行ではUser/Sessionと生存中ヒープから検証用トークンを検出し、仲介後の3実行では検出しなかった。各ダンプは約306–309 MB。macOSではOS経由の自プロセスコマンド取得は利用できず、Linuxでは取得・検査できた。これは検査した経路における合成トークンの非検出で、未知の全経路からの非開示や実アカウントによるオンライン参加の証明ではない。

LinuxはUTM 5.0.5の既存Ubuntu ARM64検証VM、kernel 6.8.0-139、bubblewrap 0.9.0、GNOME Wayland。root所有の専用probeをAppArmorの既存検証プロファイルに置き、monaユーザーで実行した。ホストの認証情報は移していない。初回は誤って指定した `--preserve-fds` が拒否されてゲーム起動前に失敗した。bubblewrapはそのオプションを持たず、継承したFDを子コマンドへ渡すため、指定を削除してIPC往復を実証した。[bubblewrap 0.9.0実装](https://github.com/containers/bubblewrap/blob/v0.9.0/bubblewrap.c)

検証時の自動チェックはmacOSでRust 118件成功・5件スキップ、Linuxで121件成功・4件スキップ、双方Clippy `--all-targets -D warnings` 成功。UIは `pnpm check` とPlaywright 42件成功。ヒープダンプの内容自体は保存・コミットせず、検査結果だけを残す。

## 署名仲介の互換性検証（2026-09-14）

Rustは2048-bit RSA鍵を起動単位で保持し、証明書のprivateKeyを不透明な識別子に置換する。Cryptの読み書きと、識別子専用のJCA ProviderをJava Agentで追加した。署名入力はMinecraftのv1チャット形式だけを許可し、起動時のUUID、セッションごとの増加する連番、現在時刻、有効期限、文字数とlast-seen署名数を検査する。全Modが許可済みのチャット形式で署名を依頼できる制約は残る。

`ChatSigningSmoke.java` とRustのignored testは、インストール済みの公式ゲーム・authlib JARを読み、実JNI・匿名IPCで別JVMからRustの合成鍵を使う。ゲーム自身のPlayerChatMessageが生成したバイト列をSignerへ渡し、JDKの通常RSA検証で署名を検査する。秘密鍵のgetEncoded/getFormatがnullであること、Cryptのキャッシュ用文字列が識別子のみになること、前回起動の識別子と連番再使用の拒否、通常RSA処理の維持も検査する。

```sh
MONALAUNCHER_CHAT_SMOKE_ROOT=/absolute/path/to/minecraft \
MONALAUNCHER_CHAT_SMOKE_JAVA=/absolute/path/to/jdk/bin/java \
MONALAUNCHER_CHAT_SMOKE_VERSION=1.21.8 \
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib \
  official_game_chat_signer -- --ignored --nocapture
```

1.21.8はJava 21、26.2はJava 25を指定する。両方をmacOSで実行し成功した。外部サービスに通信せず、テスト専用PKCS#8鍵と無効な発行者署名を使うため、公式証明書・secure profileサーバーでの成功を示さない。ゲームの画面を起動する検証とも分ける。

署名追加後のFabric起動回帰では、新しいModが実CryptクラスをFabric経由で読み込み、chatAdapterPresentを追加検査する。署名要求自体は通信OFFのため実行しない。結果は `chat-adapter-macos-2026-09-14.json`。このModのSHA-256は追加のクラス読み込み検査により以前の比較記録と異なる。

署名鍵のFabricヒープ探索と、実証明書によるオンライン接続・署名チャットは未検証。既存の秘密鍵キャッシュを取り除く移行処理も別途必要。一般向けのsecure profile対応としてはまだ表示しない。

Linuxの同じUTM検証VMでも、26.2 / authlib 9.0.75 / Java 25の公式クラスによる署名互換試験と、Fabric 0.19.5実ゲームでのCrypt差し替え・IPC・生存中ヒープの回帰検査が成功した。結果は `chat-adapter-linux-2026-09-14.json`。1.21.8は既存インストーラーがLWJGL freetype 3.3.3のLinux ARM64ネイティブを未対応として起動前に拒否したため、このVMでの成功には含めない。

追加確認では、macOSでRust 124件成功・6件スキップ、Linuxで127件成功・5件スキップ、双方Clippy成功。その後の待機中失効修正は双方のIPC 5件で検証し、Clippyも再実行した。IPCが待機中でも250 msの読取タイムアウトごとにアカウント失効を検査し、切断して鍵を持つ処理を破棄する。処理中に失効した場合も結果を返さず、分類済みの失効エラー後に閉じる。HTTP処理中の待機時間は既存の最大30秒に従う。
