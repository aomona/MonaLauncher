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

生存していないオブジェクトを含むヒープ全体、変換・分割された値、ネイティブメモリ全体は未検査。ランチャーのメモリについては後述の限定したOS APIによる読取試験を追加した。したがって `wholeHeapScanned` はfalseのままとし、生存中ヒープの検査だけを `liveJavaHeapDumpScanned` で示す。署名鍵・オンラインサーバー接続の検証も追加工程。

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

署名鍵のFabricヒープ探索は未検証。実証明書によるオンライン接続・署名チャットは後述の26.2 / macOS試験へ進み、既存の公式秘密鍵キャッシュを起動前に取り除く移行処理も追加した。他の構成のオンライン検証が揃うまで、一般向けのsecure profile対応としてはまだ表示しない。

Linuxの同じUTM検証VMでも、26.2 / authlib 9.0.75 / Java 25の公式クラスによる署名互換試験と、Fabric 0.19.5実ゲームでのCrypt差し替え・IPC・生存中ヒープの回帰検査が成功した。結果は `chat-adapter-linux-2026-09-14.json`。1.21.8は既存インストーラーがLWJGL freetype 3.3.3のLinux ARM64ネイティブを未対応として起動前に拒否したため、このVMでの成功には含めない。

追加確認では、macOSでRust 124件成功・6件スキップ、Linuxで127件成功・5件スキップ、双方Clippy成功。その後の待機中失効修正は双方のIPC 5件で検証し、Clippyも再実行した。IPCが待機中でも250 msの読取タイムアウトごとにアカウント失効を検査し、切断して鍵を持つ処理を破棄する。処理中に失効した場合も結果を返さず、分類済みの失効エラー後に閉じる。HTTP処理中の待機時間は既存の最大30秒に従う。

## Fabric Modからのネイティブな親メモリ読取（2026-09-14）

`native_memory.c` と `NativeMemoryProbe.java` を追加。ハーネスが保持する合成トークンの正確なアドレス・長さと親PIDを専用インスタンスの設定に渡し、ModからOS APIでその範囲を読み出す。実アカウントの資格情報を対象にしない。一部だけでも読めた場合は読取成功として扱い、トークン検出または親メモリ読取成功でハーネスを失敗させる。読み出したバイトは既存のSHA-256比較にも渡す。Linuxのビルド時には、読取可能ページと読取禁止ページをまたぐ要求で8バイトだけ返る対照試験を行い、部分読取を拒否と誤判定しないことも検査する。JSONにはPID・アドレス・読んだ内容を残さない。

ビルドにはJDKのJNIヘッダーとCコンパイラーが必要。`build.py` はホストOS用のネイティブライブラリをJARへ同梱し、同じOS APIによる自プロセスメモリの読取をサンドボックス外で実行して成功を必須にする。Modは一時ファイルへ展開してJNIを読み込み、検査後にファイルを削除する。ハーネスも異常終了時の生成ファイルを片付ける。

- macOSは親の `task_for_pid` と `mach_vm_read_overwrite` を使用する。自プロセスの対照には `mach_task_self()` を使用する。[Apple API](https://developer.apple.com/documentation/kernel/1402127-mach_vm_read_overwrite)
- Linuxは `process_vm_readv` を使用する。bubblewrap内から指定するPIDはホスト側のPIDであり、PID名前空間内で同じ対象を指すとは扱わない。現在のseccompはこのシステムコール自体をEPERMにするため、自プロセス宛ての読取拒否も別途必須にする。[Linux API](https://www.man7.org/linux/man-pages/man2/process_vm_readv.2.html)

ハーネスは `nativeMemoryProbeRan=true`、`nativeControlPassed=true` と親読取拒否を必須にする。macOSは自プロセス読取成功・親KERN_FAILURE(5)、Linuxは自プロセス・親の双方EPERM(1)を検査する。結果は `native-memory-2026-09-14.json`。

これは指定したOS APIでの読取試験である。macOSの拒否がSeatbeltだけに由来するとは断定しない。ネイティブメモリ全体の走査、他のデバッグ・IPC経路、カーネルやランチャーの脆弱性は検証していない。以前の生存中Javaヒープ検査と区別し、未知の全経路からの非開示を証明したとは扱わない。

## 起動構成の互換性判定（2026-09-14）

バックエンドは1.21.8 / authlib 6.0.58 / Java 21と26.2 / authlib 9.0.75 / Java 25を照合し、VanillaまたはFabric 0.19.5に限定する。メタデータだけでなくauthlib JARのサイズとSHA-256を検査する。未知の構成、Java不一致、authlib差し替え・重複・破損を拒否し、実トークンの直接受け渡しへ戻さない。

`compatibility-2026-09-14.json` にRustソースのハッシュと、判定追加後のmacOS 1.21.8・26.2 / Linux 26.2の実Fabric試験を記録。3実行とも合成トークン非検出、IPC往復成功、限定した親メモリ読み取り拒否を確認した。Linux ARM64の1.21.8、Windows、オンライン参加はこの成功に含まない。

## 実オンライン検証サーバーの準備

`python3 tools/auth-broker-probe/prepare_online_server.py` は公式26.2サーバーを固定URLから取得し、サイズとSHA-1を検証する。出力は無視対象の `build/online-server-26.2/`。`127.0.0.1:35565` のみで待ち受ける設定とし、`online-mode=true`、`enforce-secure-profile=true`、RCON・query無効で準備する。既存の異なる設定は上書きせず停止する。

このスクリプトはサーバーを起動せず、新規の `eula.txt` を `eula=false` にする。実行には利用者による [Minecraft EULA](https://www.minecraft.net/en-us/eula) への同意が必要。実アカウントによる接続・署名チャットの検証は未実施で、合成トークン用の読取ハーネスだけでは代替できない。2026-09-14に取得・ハッシュ検証・再実行時の設定維持を確認した。

## キャッシュ移行の攻撃側検証

起動前に合成トークンを `profilekeys/legacy-auth-probe.json` へ置く。起動処理は公式の再取得可能な `profilekeys` ディレクトリを、認証権限のON/OFFやログイン状態によらず削除する。削除できなければ起動しない。Modが確認する `legacyProfileKeyCacheVisible=false` を既存の非検出・IPC・ヒープ・ネイティブ読取試験に追加した。Rustでは全アカウント分の削除、通常ゲームファイルの保持、リンク先の保持、想定外のファイルによる失敗を検査する。

この処理は既存の鍵を復元不能に消去したり、Modが別の場所へ複製した鍵を回収したりするものではない。ディレクトリ内のリンクをたどらない削除には [Rustのremove_dir_all](https://doc.rust-lang.org/std/fs/fn.remove_dir_all.html) を使用する。

## 実アカウントによる26.2接続試験

実アカウントの試験は明示的なignored testとして分離した。準備済みの26.2サーバーを、利用者がEULAに同意した後で起動し、既存ランチャーでサインインを済ませる。キーチェーンの許可が求められた場合は利用者が操作する。トークン・秘密鍵・アカウント識別子は検証結果に出力しない。

```sh
MONALAUNCHER_ONLINE_ROOT="/absolute/path/to/minecraft" \
MONALAUNCHER_ONLINE_PROBE_JAR="/absolute/path/to/mona-token-read-probe.jar" \
MONALAUNCHER_ONLINE_SERVER="/absolute/path/to/online-server-26.2" \
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib \
  saved_account_joins_secure_profile_server_and_sends_chat -- --ignored --nocapture
```

テストは専用の `auth-online-26-2` インスタンスで通信と認証仲介を有効にし、通常のアカウント更新・仲介・サンドボックス起動処理を使用する。Modは26.2の実ゲームから `127.0.0.1:35565` へ接続し、実チャットセッション内の秘密鍵が `RemotePrivateKey` でエンコード不能であること、公式キャッシュに秘密鍵PEMがないことを確認する。通常の `sendChat` で固定の検証文を1回送信し、接続継続を確認する。Rust側は今回の起動後に増えたサーバーログだけを検査し、検証文が受信され、`Not Secure` と判定されていないことを確認する。

サーバー26.2の公式クラスを静的確認したところ、署名がない・サーバー側で期限切れのメッセージは `PlayerList.verifyChatTrusted` から `MinecraftServer.logChatMessage` を経由して `Not Secure` 付きで記録される。静的確認自体は実通信成功の証拠ではない。オンライン試験は合成トークン探索とは別の検証で、実秘密鍵の全ヒープ探索を行ったことにはならない。

2026-09-14の実行結果は `online-macos-26.2-2026-09-14.json`。macOS / Java 25 / authlib 9.0.75 / Fabric 0.19.5で、実アカウントによる2回の個別起動・接続と、サーバーが信頼済みとして扱ったチャットの受信が成功した。検証後はゲーム・サーバーを終了した。キャッシュ移行を含む3構成の合成トークン試験は `cache-migration-2026-09-14.json`。
