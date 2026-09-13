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

現行ハーネスは `tokenDetected=false`、`agentAdapterPresent=true`、`brokerHandshakeCompleted=true` を合格条件とする。ゲーム起動中の実authlib呼び出しでRust仲介とhelloを交換し、通信OFFのため外部認証要求は拒否される。これはオンライン参加成功の試験ではない。

正の対照はコミット `fd02af8` の同じハーネスで再現できる（別worktreeで実行する）。そこでは直接渡したランダムな検証用トークンをUser/Sessionから実際に検出した。現行の製品コードには実トークンを直接渡す検証用フォールバックを残さない。

## 検査範囲

- 実User/Sessionの文字列フィールド。
- Minecraft/authlibの到達可能なオブジェクトフィールド（深さ5・2万オブジェクトまで）。
- JVM起動オプション、OSが公開する自プロセスコマンド、環境変数、システムプロパティ。
- 専用ゲームディレクトリの通常ファイル（深さ4・512件・1件256 KiB・合計8 MiBまで）。リンクは追跡しない。

Javaヒープ全体・ネイティブメモリ・ランチャーのメモリはこのMod単体では未検査。OSが返さないプロセス情報は空となり、その面から検出できないことはOS情報の非公開も含む。署名鍵・認証仲介・オンラインサーバー接続の検証は追加工程。

2026-09-13のmacOS Seatbelt実行では、1.21.8と26.2の双方でFabric entrypointが実行され、実User/Sessionから検証用トークンを検出。検査結果は `baseline-macos.json` に保存。

仲介後のmacOS実ゲーム結果は `brokered-macos.json` に保存。秘密鍵取得とチャット署名は未実装のため、現在のアダプターは未対応エラーを返す。Windows IPCは未実装、Linux経路は実ゲーム未検証。どちらもmacOSの結果を根拠に対応済みとしない。
