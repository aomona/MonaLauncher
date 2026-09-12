# macOS Seatbelt実験対応

既存のTauri起動コマンドから、macOSでは`/usr/bin/sandbox-exec`経由でMinecraftを開始する。Windowsは従来のAppContainerを使う。隔離を外した通常起動へのフォールバックは設けない。Linuxのランチャー組み込みは今回の対象外。

## 試し方

```sh
pnpm tauri dev
```

このマシンには`macOS Seatbelt Demo`（Minecraft 1.21.8、Vanilla、デモ）をアプリのデータ領域に作成済み。ランチャーのPlayから起動できる。新規インストール時はデモのVanilla 1.21.8を選ぶ。Microsoft認証と製品版は今回検証していない。

インストーラはAdoptiumのmacOS/ホストCPU向けJDKを取得し、SHA-256を照合して管理領域へ配置する。macOSの`Contents/Home/bin/java`とtar.gz展開に対応した。アーカイブのリンク・特殊ファイル・展開サイズ超過は拒否する。MinecraftのOSルールには`osx`を使い、クラスパスはコロンで区切る。

UI操作なしで同じインストーラ・起動関数を検証する場合:

```sh
cargo run --manifest-path src-tauri/Cargo.toml --locked --bin minecraft_smoke --   "$HOME/Library/Application Support/me.aomona.monalauncher/minecraft" 90
```

このコマンドは専用デモを必要な場合だけインストールし、ウィンドウを起動する。既存インスタンスの設定は上書きしない。指定秒数（10〜600秒）後に自分が起動したプロセスを終了する。終了コード0は観測期間の生存と停止処理の成功を意味し、ゲームプレイの成功を意味しない。UIから同じインスタンスを実行中には使わないこと。

## 許可する範囲

- 選択した管理下Java、共有ライブラリ、アセット、当該バージョン、起動用ファイルは読み取り。
- 当該インスタンスのgameと、ランチャーが毎回作成する専用tmpだけに書き込み。
- `HOME`とJavaの`user.home`はgameへ向け、Java/JNA/LWJGL/Nettyの展開先を専用tmpへ統一。
- 親の環境変数を消去し、PATH・HOME・TMPDIR・LANGだけを設定。標準入力は閉じる。
- WindowServer・フォント・GPU関連の列挙したMachサービスとIOKitクラスを許可。実ゲームで必要になった`com.apple.MTLCompilerService`をXPC名で追加。
- IP通信・ホストUnixソケットの許可は追加しない。game以外の個人ファイルや他インスタンスの内容は公開しない。ファイルのメタデータ読み取りは広く許可する。
- 通常の停止要求では専用プロセスグループを終了し、一時領域を削除する。

Machサービスを経由するアクセスは、直接のファイル/ソケット制限とは別の監査が必要。Windows Job Objectと同等の、ランチャー異常終了時やプロセスグループを離脱した子の強制終了は未実装。これらを含む完成した安全性の証明ではない。

## 2026-09-12の実機検証

環境はApple Silicon macOS（Darwin 25.1.0）、Temurin JDK 21.0.12.1+1 ARM64、Minecraft 1.21.8、LWJGL 3.3.3+5。管理下JDKのパッケージSHA-256は`3623232f33a9c3baadf304480b2535f9a3cba8a58d42ecbb438ba267315d9998`。

1. 最初の実行は、MinecraftのJVM引数がJNAの展開先を読み取り専用nativesへ向けていたため失敗。専用tmpへの上書きで解消。
2. 次の実行はMetalコンパイラ接続が拒否され、GPUドライバのコンパイル処理でSIGABRT。対象XPCサービスだけを追加して解消。
3. 最終プロファイルで90秒間生存し、ランチャーと共通の停止処理が成功。再実行ではOSのウィンドウ情報でMinecraftのウィンドウを確認し、そのウィンドウだけを撮影してデモのタイトル画面を目視確認した。マウス・キーボードによるゲーム操作は行っていない。

音声初期化は拒否され、Minecraftはサウンドを無効化して起動した。音声出力、マイク、ナレーター、ゲーム内入力、ワールド生成、保存、MOD、製品版、別のmacOS/Intel Macは未検証。外部サービスの名前解決失敗ログも残る。ネットワークを許可して解消する変更はしていない。

確認コマンド:

- `cargo test --manifest-path src-tauri/Cargo.toml --lib --locked`: 65件成功。実Seatbeltでの許可ファイル、禁止ファイル、リンク経由アクセス、子プロセスの書き込み制限、tarのリンク拒否と実行権限も含む。
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked`: 成功。既存のWindows専用token store/probeにmacOS上の未使用警告が残る。
- `pnpm check`: 成功。
- `pnpm test:ui`: 17件成功。TauriをモックしたUIテストであり、実UIのPlayボタン操作やMicrosoft認証の証拠ではない。
- `pnpm tauri build --debug --bundles app`: macOSアプリのビルド成功。

実ゲームの起動検証は`minecraft_smoke`から共通バックエンドを呼んだもの。UIを通した起動・停止の一連の操作は別途確認が必要。

## 実装参考

- [Adoptium API cookbook](https://github.com/adoptium/api.adoptium.net/blob/main/docs/cookbook.adoc): OS/CPU別ランタイム取得。
- [ChromiumのGPU用Seatbeltプロファイル](https://chromium.googlesource.com/chromium/src/+/main/sandbox/policy/mac/gpu.sb): MetalコンパイラのXPCサービス指定。今回の追加はローカルの拒否ログとクラッシュスタックでも裏付けた。
- [先行した独立検証](../tools/sandbox-lab/README.md): 今回はそこでのJava/LWJGL確認をランチャーへ組み込んだ。
