# アクセストークンをゲームへ渡さない認証仲介の計画

作成日: 2026-09-13。調査対象: `ff73a80`。設計案であり、実装・実ゲーム検証は未実施。

## 目的と保証の範囲

MinecraftアクセストークンをRust側のメモリだけで保持し、ゲームには認証結果だけを返す。Javaの起動引数・環境変数・Sessionオブジェクト・Java Agent・ゲームから読めるファイル・IPC応答へ実トークンを入れない。Microsoftアクセストークンと更新トークンも引き続きゲームへ渡さない。

ゲームと全Modを一つの信頼できないプロセス群として扱う。Java側の改変防止を安全性の根拠にせず、秘密と操作の許可判定をOSサンドボックスの外に置く。ホストOS・ランチャー・管理Javaランタイムが侵害されていないことが前提。

この方式で防ぐのは、実トークンを盗み、ゲーム終了後に別環境から再利用する経路。起動中の悪意あるModによる許可済み認証操作の呼び出しや、外部から受け取った要求の中継までは防げない。特に `joinServer` のserver hashはModも指定できるため、仲介だけで「本人が選択したサーバーへの接続」とは証明できない。レート制限や要求IDも本人操作の証明にはならない。

サーバー接続先やハンドシェイクまで信頼境界の外で検証するには、別途ゲーム通信そのものの仲介が必要。その場合も悪意あるModからの操作を完全に判別できるとは扱わず、今回のトークン非開示と分ける。

## 確認した現状

- `src-tauri/src/auth/minecraft_services.rs`: MinecraftSessionに実トークンを保持。
- `src-tauri/src/commands/auth.rs`: 認証・更新をsingle-flight化し、セッションを期限付きでキャッシュ。現在のサインアウトはキャッシュと保存済み更新トークンを消す。
- `src-tauri/src/commands/minecraft.rs`: `accessToken` がONなら実トークンをMinecraftIdentityへ複製。
- `src-tauri/src/minecraft/launcher.rs`: 実トークンを `${auth_access_token}` へ展開。ここを置き換える。
- 通信許可はInternet/LAN全体のON/OFFであり、宛先別フィルターではない。Windowsのloopbackは自動解除していない。
- Windowsの起動処理はstdout/stderrハンドルだけを明示継承。Linuxはbubblewrapのnamespaceとseccompを使用し、macOSはSeatbeltを使用。認証用の双方向IPCはまだない。
- 管理済みの公式authlib 6.0.58（Minecraft 1.21.8）と9.0.75（26.2）を `javap` で確認。双方に `joinServer(UUID, String, String)`、User APIのプロパティ・ブロック一覧・鍵取得・通報操作がある。`KeyPairResponse.KeyPair` は秘密鍵文字列を含む。JARのSHA-1はローカル公式バージョンメタデータの値と一致した。これは静的なAPI確認であり、仲介の動作証明ではない。

## 採用する構成

```mermaid
flowchart LR
  subgraph sandbox[ゲームのOSサンドボックス]
    game[Minecraft / 全Mod]
    adapter[Java認証アダプター]
    game -->|実トークンを持たない| adapter
  end
  subgraph trusted[ランチャー側]
    broker[Rust AuthBroker]
    vault[認証セッション / 実トークン]
    broker --> vault
  end
  adapter -->|専用IPC: 定義済み操作だけ| broker
  broker -->|Rust側で認証情報を付与したHTTPS| mojang[Minecraft公式認証サービス]
  broker -->|検査済みの結果| adapter
```

Java Agentでauthlibの対応メソッドを起動時に差し替え、Rust AuthBrokerの操作へ変換する。ゲームのトークン欄は公式サービスでは認証できない固定プレースホルダーとする。プレースホルダー自体には仲介権限を持たせない。

通常のHTTPSプロキシ設定だけでは、CONNECTトンネル内の暗号化された認証情報を置換できない。TLSを復号する方式では独自CA・信頼ストア・複数HTTP実装への対応も必要になるため、今回は認証API単位で仲介する。ゲームサーバー向けTCP通信は既存のネットワーク権限に従う。[Java Networking](https://docs.oracle.com/en/java/javase/21/core/java-networking.html)

### IPC

第一候補は起動ごとに作る専用の継承パイプ2本による双方向通信。stdout・stderr・ナレータープロトコルとは混在させない。待受TCPポートを作らず、公開可能な仮トークンだけで第三者が仲介を利用できる構成にしない。

- Windows: `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` に認証用の子側ハンドルだけを追加。親側ハンドルの継承禁止、起動失敗時の閉鎖、別AppContainerからの複製・利用拒否を実機検証する。
- macOS: 子側FDだけを継承し、Seatbelt下の専用IPCが動作するか検証する。親プロセスのメモリ・他のFDへのアクセスを許可しない。
- Linux: bubblewrapによるFD保持とexec後のFD配置を実証する。既存のseccomp用FDと混同せず、必要なIPCのためにhost network共有を追加しない。
- JavaからのネイティブHANDLE/FD操作は小さなJNIブリッジに限定する。秘密は置かない。非公開JDK APIへの依存を避ける。
- 固定スキーマ・長さ付きフレーム・要求ID・読み書き期限・同時要求数上限を定義。起動nonceは取り違え検知用であり、同じゲーム内のModから秘密にできる値とは扱わない。
- FD/HANDLE継承が特定OSで成立しなければ、そのOSは未対応とする。代案のnamed pipe/Unix socketはアクセス制御・接続元・寿命を個別検証してから採用する。Windows named pipeも名前の秘匿だけに頼れない。[Named Pipes](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipes)

### Rust AuthBroker

ゲームから任意のURL・HTTPメソッド・ヘッダー・アクセストークン・別アカウントIDを受け取る汎用プロキシにはしない。各操作について宛先・メソッド・本文・返却フィールドをRust側で組み立てる。

| 操作                          | 入力・振る舞い                                                                      | 返すもの                           |
| ----------------------------- | ----------------------------------------------------------------------------------- | ---------------------------------- |
| JoinServer                    | server hashを形式・サイズ検査。UUIDと実トークンは起動時に固定したアカウントから取得 | 成功または分類済みエラー           |
| GetUserProperties             | 起動アカウントの実際の権限・制限を取得。マルチプレイ禁止等を許可に書き換えない      | 必要な公開プロパティ               |
| GetBlockList                  | 起動アカウントのブロック一覧を取得。短時間キャッシュ                                | UUID一覧                           |
| GetChatCertificate / SignChat | 後述の独立工程で実装                                                                | 公開証明書・署名。秘密鍵は返さない |

宛先は固定の公式HTTPSエンドポイントだけ。証明書検証を有効にし、HTTP redirect・ユーザー指定proxy・環境由来proxyを無効化する。本文・応答サイズ、timeout、キュー長を制限。エラー本文やHTTPヘッダーをそのままゲームへ返さず、許可したフィールドへ変換する。通報・テレメトリ・Realms・スキン更新等は操作を個別設計するまで追加しない。未対応操作は明示的な未対応結果とし、架空の成功を返さない。

セッションを `launch_id + instance_id + account_id + account_generation` に固定する。ゲームからアカウントを選ばせない。トークン更新は既存single-flight処理を再利用するが、アカウント切替後に別人のトークンを返さないよう認証管理を分離する。

ゲーム終了・起動失敗・IPC切断・ランチャー終了で仲介セッションを破棄。サインアウト・アカウント切替でgenerationを進め、進行中の更新結果やHTTP応答も破棄する。要求の実行直前と応答返却直前にgenerationを検査する。既に公式サービスで実行された処理や、成立済みのゲーム接続をサインアウトで取り消せるとは扱わない。

### 署名付きチャット

`joinServer` だけの仲介で全マルチプレイ対応とはしない。公式1.19.1リリースで署名付きチャットと `enforce-secure-profile=true` の既定化が示されている。[公式リリースノート](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-19-1)

鍵取得APIのレスポンスを丸ごとゲームへ戻すと、アクセストークンは隠せてもチャット用秘密鍵はゲームへ出る。今回の推奨設計では秘密鍵もRust側に保持し、ゲームの署名処理を追加アダプターで仲介する。署名対象の形式・セッション・時刻・連番・サイズ・期限を検証する。ただし、許可された形式の悪意あるチャット署名まで本人操作と区別できるとは保証しない。

公開証明書への置換だけでは、ゲームが要求する秘密鍵オブジェクトとの互換性は得られない。Java側の鍵生成・キャッシュ・署名呼び出し箇所を対象バージョンごとに確認し、秘密鍵キャッシュが作られないことも確認する。この工程が終わるまでsecure profile対応を表示しない。チャット署名キーの非開示はトークン非開示より追加の範囲であり、工程と検証結果を分けて報告する。

## パーミッションと互換性

提案: 既存の直接受け渡しを「アカウント認証の仲介」に置き換える。初期値は引き続きOFF。内部は `accountAuthentication: disabled | brokered` とし、直接受け渡しへの自動フォールバックを廃止する。

- 旧 `accessToken=false` または項目なし → disabled。
- 旧 `accessToken=true` → brokered。未対応OS・バージョンでは理由を示し、トークンを直接渡して起動しない。
- 新旧項目が同時にあって矛盾する設定は保存・移行で拒否する。移行処理は既存のstrictなserde検査と整合させる。
- 通信権限は独立して保存する。初期実装の認証仲介は通信ONのときだけ有効とし、通信OFFなら設定を保持して外部認証要求を拒否する。設定変更で通信を自動許可しない。
- デモ・未ログイン時は仲介セッションを作らない。
- 説明文には「実トークンを渡さず認証を仲介する」「全Modも起動中は許可された認証操作を利用できる」「次回起動から適用」を明記。
- 対応するMinecraft/authlib/Java/loaderの組をバックエンドの互換表で判定。未知の構成は未対応とする。任意のMod・別のHTTPクライアント・認証差し替えModの互換性は自動保証しない。

## 実装順序と完了条件

1. **互換性とIPCの実証**: 最初は1.21.8 / authlib 6.0.58 / Vanillaで、認証呼び出しと署名・キャッシュ経路を一覧化。3 OSそれぞれでゲームサンドボックスから専用IPCの往復、別インスタンス拒否、終了時切断を実証する。26.2 / authlib 9.0.75は次の互換対象。ここで成立しないOSは保留。
2. **Brokerの土台**: `src-tauri/src/auth/broker/` に操作型・起動セッション・寿命・レート制限・公式APIクライアントを実装。認証管理からトークンをゲーム起動コードへ返さないAPIへ変更。モックHTTPで秘密非開示、redirect拒否、過大入力、失効と更新競合を検査。
3. **Javaアダプターと最小接続**: `src-tauri/java/auth-bridge/`、`build.rs`、launcher、3 OSの起動処理を接続。AgentとJNIは読み取り専用のランチャー管理領域に配置。実トークンの展開経路を削除し、brokered起動失敗時に直接渡さない。最初は管理された検証サーバーでonline-mode参加を確認し、secure profile未対応の試作であることを明示。
4. **署名と必要User API**: 秘密鍵保持・署名仲介・権限制限・ブロック一覧を追加。`online-mode=true` かつ `enforce-secure-profile=true` の検証サーバーで参加と署名付きチャットを確認。鍵期限切れ・再接続・複数インスタンスを検査。Fabricを加えて同じ検査を実行する。
5. **設定移行と画面**: permissions、IPC型、既存Permissions部品を変更。保存失敗・旧設定・未対応表示・通信OFF・未ログインを確認。デザイン定義とガイドを更新し、既存トークンを使用する。
6. **攻撃側からの検証と公開判定**: 実トークンを探す試験Mod・ネイティブ試験コードで、引数・環境・Javaヒープ・読み取り可能ファイル・IPC・ログを確認。別インスタンスや別プロセスからの利用、ランチャーメモリ読取、終了後再利用を各OSで検査。成果を「トークン非開示」「署名鍵非開示」「限定操作の悪用余地」「実接続成功」に分けて記録する。

各工程は意味のある単位でコミットし、UI変更時は `pnpm check` と `pnpm test:ui`、Rustはfmt/test/clippy、Java/JNIはビルドとプロトコル・実プロセステストを実施する。IPCモックやJARの静的確認を、Windows AppContainer・macOS Seatbelt・Linux bubblewrapでの実機成功の代わりにしない。

## リリース判定

対応表に記載する各構成で、既定OFF、旧設定移行、ON時の実トークン非開示、実online-mode接続、secure profileとチャットの状態、サインアウト失効、複数インスタンス分離、破損・未知アダプターでの安全な停止が確認できること。

Realms、認証を独自実装するMod、通報・テレメトリ等の追加APIは次段階。サーバーの安全設定を変更しなければ接続できない状態を、一般向けマルチプレイ対応の完了にしない。既に直接受け渡しで起動しているゲームからトークンを回収することはできないため、移行の保護は新方式での次回起動から有効。

参照した公式配布物: [authlib 6.0.58](https://libraries.minecraft.net/com/mojang/authlib/6.0.58/authlib-6.0.58.jar)、[authlib 9.0.75](https://libraries.minecraft.net/com/mojang/authlib/9.0.75/authlib-9.0.75.jar)。配布物はリポジトリへ追加しない。
