# UTM 4.7.5のM4 Pro GPU検証

2026-09-12、既存のUbuntu ARM64 VMでUTM 4.7.4から4.7.5への更新後を比較した。ランチャーの実装は `8651d3c`。VM停止中に設定を退避し、表示デバイスを `virtio-gpu-gl-pci`、UTMの `QEMURendererBackend` をMetal（2）とOpenGL（1）に切り替えた。ゲストMesaは25.2.8。GPU試験では `LIBGL_ALWAYS_SOFTWARE` を設定していない。

## 観測

| 経路                 | renderer                                | Accelerated | ゲストのOpenGL                  |
| -------------------- | --------------------------------------- | ----------- | ------------------------------- |
| VirGL → ANGLE Metal  | `ANGLE Metal Renderer: Apple M4 Pro`    | yes         | 2.1、core profile 0.0、GLES 3.0 |
| VirGL → ANGLE OpenGL | `Apple M4 Pro, OpenGL 4.1 Metal - 90.5` | yes         | 2.1、core profile 0.0、GLES 3.0 |

両方でGNOMEとゲストエージェントが起動した。Metal経由の `glxgears` も同じGPU rendererを報告した。以前のGPU設定時の起動不調はこの実行では再現していないが、UTM更新とrenderer明示変更の両方を行ったため、改善の原因を更新だけに帰属しない。

Metal経由の本体Minecraft起動プローブは、次のエラーで失敗した。

```text
Failed to create backend OpenGL
Driver does not support OpenGL 3.3
```

終了処理では追加で `libopenal.so` のSIGSEGVも出たため、音声まで成功した実行とは扱わない。

OpenGL経由では、サンドボックス外のLWJGL 3.3.3プローブでOpenGL 3.3 core contextを要求したところ、`GLFW_VERSION_UNAVAILABLE` / `GLXBadFBConfig` で失敗した。同じプログラムに `LIBGL_ALWAYS_SOFTWARE=1` を付けると、llvmpipeでウィンドウ作成とピクセル読戻し（RGBA 64,128,191,255、glError=0）が成功した。使用ソースは既存 `LwjglProbe.java` の要求minor versionを2から3へ変更したものをゲストの `/tmp` でコンパイルした。

ホストのrenderer文字列にあるOpenGL 4.1は、Linuxゲストが4.1を利用できるという意味ではない。M4 ProのGPUアクセラレーションは認識されているが、このUTM構成がゲストへ公開するAPIはMinecraft 26.2の要求を満たさない。サンドボックス外でも再現するため、権限を広げる修正は行っていない。APIのバージョンを偽装する環境変数も使っていない。

## 後処理と限界

- VMを通常停止し、表示を試験前の `virtio-ramfb` へ復元。UTMのrenderer設定も試験前の既定値へ戻した。
- VMの起動を連続操作した際にUTM本体のSIGSEGVとAppleEventタイムアウトも観測した。完全終了後の再起動で比較を続行できた。このエラーをOpenGL context失敗の原因とは断定しない。
- 生ログはGit対象外の `reports/utm475-*`。ホストの入力キャプチャやマイク録音は行っていない。
- UTMの別バージョン・他の仮想化ソフト・実Linux GPUの可否を、この結果から一般化しない。

## 参照

- [UTMのGPU対応表示デバイス](https://docs.getutm.app/settings-qemu/devices/display/)
- [UTMのrenderer選択](https://docs.getutm.app/preferences/macos/)
- [UTM 4.7.5のrenderer列挙値](https://github.com/utmapp/UTM/blob/v4.7.5/Services/UTMQemuSystemBackends.h)
