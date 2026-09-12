# UTM 5.0.5 + Apple Core OpenGLのM4 Pro検証

## 構成

2026-09-12、UTM 4.7.5でOpenGL 3.3を作成できなかった既存のUbuntu VMを、UTM 5.0.5 BetaのApple Core OpenGLで再検証した。ゲーム・サンドボックスのコードは `8651d3c` のまま。

- 公式配布: `https://github.com/utmapp/UTM/releases/download/v5.0.5/UTM.dmg`
- SHA-256: `713afe73c711f01344b8766654be531cd391ed2e30931206f43b5159f143764f`（GitHub公開digestと一致）
- 配置先: `/Applications/UTM.app`、バージョン5.0.5。コード署名のdeep/strict検証成功。
- 表示デバイス: `virtio-gpu-gl-pci`
- UTM renderer: **Apple Core OpenGL**（`QEMURendererBackend=3`）。ANGLEのOpenGL選択とは異なる。
- Ubuntu 24.04.5 ARM64 / Mesa25.2.8 / X11、ホストGPU Apple M4 Pro
- `LIBGL_ALWAYS_SOFTWARE` やOpenGLバージョンの上書き変数を付けずに実行。

試験前にVM全体を停止状態でAPFSコピーし、UTM 4.7.5アプリと設定も退避した。退避先はローカルの `~/Library/Application Support/MonaLauncherValidation/utm-5.0.5-backup/`。VMディスク・アプリ・個人設定をGitへ追加していない。

## OpenGLの確認

`glxinfo -B` は以下を報告した。

```text
Device: virgl (Apple M4 Pro)
Accelerated: yes
Max core profile version: 4.1
Max compat profile version: 4.1
OpenGL core profile version string: 4.1 (Core Profile) Mesa 25.2.8
```

サンドボックス外でLWJGLから3.3 core contextを要求し、ウィンドウ作成とピクセル読戻しが成功した。rendererは `virgl (Apple M4 Pro)`、読戻しRGBAは64,128,191,255、glError=0。4.7.5で失敗した同じプローブがGPU経由で成功した。

## Minecraftと権限の確認

本体の起動処理を使う `linux_minecraft_smoke` でMinecraft 26.2 Vanilla demoを60秒実行し、以下のすべてが成功した。

- OpenGL 4.1で起動し、854×480のタイトル画面を目視確認。
- Sound engine startedとJavaのPulseAudioストリームを確認。
- ナレーター要求を受信し、eSpeakプロセス開始1・正常完了1。
- 観測後のプロセス停止を含め `SMOKE PASS`。
- 同じVMで共通ポリシーの境界プローブも再実行し、書き込みON/OFF・ホスト領域アクセス拒否・直接通信拒否・呼び出し元スレッド終了後の生存・子プロセス終了が成功。

音声についてはエンジン・ストリーム・ナレータープロセスの観測であり、今回新たな聴取確認は行っていない。

## ランチャーのWebKit表示

新しいGPU設定では、通常起動のランチャーで文字が欠けた。起動時に **`WEBKIT_DISABLE_DMABUF_RENDERER=1`** を指定すると正常に表示された。この回避は専用VMでの起動環境に限定し、本体の全Linux環境向けに強制していない。Linux sandboxの環境変数は明示リストで構築されるため、この変数はゲームへ渡らない。

```sh
WEBKIT_DISABLE_DMABUF_RENDERER=1 /usr/bin/monalauncher
```

`LIBGL_ALWAYS_SOFTWARE=1` は指定しない。ランチャーはその診断変数をゲームへ引き継ぐので、指定すると今回のGPU描画を検証できない。

この回避を指定した本体UIのHome → Playからも起動し、OpenGL 4.1と音声エンジン初期化、タイトル画面、38秒後のJavaプロセス生存を確認した。Quit Gameで終了し、Javaプロセスの消滅とランチャーのPlay表示への復帰も確認。証拠画像は `reports/utm505-native-game.png`。ワールド内のプレイ性能やFPSは今回測定していない。

試験後はUTM 5.0.5と成功したGPU設定を保持した。ゲームは終了し、VMは非表示のままランチャーを起動している。4.7.5へ戻す場合は、VM停止・UTM終了後に退避したアプリと設定を戻す。ディスク形式等の互換性問題があれば、試験前のVM全体のコピーも利用できる。

## 範囲

これはM4 Proを使うUTMの仮想GPU経路の結果であり、実Linuxマシン上のAMD・Intel・NVIDIAドライバーの検証には代わらない。ベータ版の長時間安定性、全Minecraftバージョン、Modやシェーダーの互換性も未確認。ゲームへ追加の権限を与える変更は行っていない。

ローカルのログと画像はGit対象外の `reports/utm505-*` に保存。

## 参照

- [UTM 5.0.5公式リリース](https://github.com/utmapp/UTM/releases/tag/v5.0.5)
- [Apple Core OpenGL追加のPR](https://github.com/utmapp/UTM/pull/7576)
- [5.0.5のrenderer列挙値](https://github.com/utmapp/UTM/blob/v5.0.5/Services/UTMQemuSystemBackends.h)
