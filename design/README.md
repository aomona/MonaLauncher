# MonaLauncher デザイン実装ガイド

UIを実装・変更するエージェントは、このガイドを最初に読み、対象画面の原本の章を確認すること。

## 正本と今回の整備範囲

- [デザイン定義 v1.1](MonaLauncher_Design_Definition_v1.1.md): 画面構成、操作、例外、受入条件の正本。
- [デザイントークン v1.1](MonaLauncher_Design_Tokens_v1.1.json): 色・寸法・文字・動きの値。プロジェクト固有のJSON形式。
- このガイド: JSONからTailwindへの対応、実装例、作業手順。矛盾を見つけた場合、既存画面の見た目で埋めず、定義本文の用途と例外を確認する。

原本2ファイルは受領した内容をそのまま保管し、formatterの対象外にしている。原本中の製品要件は設計資料であり、今回すべての画面や機能を実装したという意味ではない。作業範囲・実行権限はユーザーの依頼に従う。

フロントエンドは新デザインへ移行済み。Home、Instances、Settingsの外観・アカウント管理、狭幅Drawer、大型Instance Modal、作成・認証・Mod管理のDialogを実装している。Themeと、このUIで観測した起動履歴はローカルに保存する。既存バックエンドの作成・起動・停止・改名・削除・診断・修復・認証・Mod操作は `src/app/useLauncher.ts` に分離して接続している。

News、Gallery、Resource Packs、Shader Packs、Worlds、Serversは取得APIがないため、未取得・未対応として表示する。Group管理、Created日時によるSort、バージョン変更、Open Folder、Java等のグローバル設定も未対応。架空の記事・画像・件数や、実行できない操作は追加しない。これはデザイン仕様全機能の完成宣言ではない。

## ファイルと更新手順

| ファイル                            | 役割                                                                                       |
| ----------------------------------- | ------------------------------------------------------------------------------------------ |
| `src/styles/index.css`              | アプリから読み込む入口。Tailwind、フォント、生成テーマ、共通コンポーネントCSS、補助utility |
| `src/styles/theme.generated.css`    | Tailwind v4のCSSベース設定。`tailwind.config.js` は使用しない                              |
| `scripts/generate-design-theme.mjs` | JSONからCSS変数・`@theme`・動きのutilityを生成                                             |
| `scripts/design-theme.test.mjs`     | 実際のTailwindコンパイラでutility生成を確認                                                |

1. 必要な変更が仕様変更なのか、既存仕様への実装合わせなのかを区別する。
2. 仕様変更が依頼された場合、本文・JSON・このガイドを整合させる。
3. `pnpm design:generate` でCSSを再生成する。生成ファイルは直接編集しない。
4. `pnpm check` で生成物の同期、utility生成、書式、lint、型、production buildを確認する。
5. 画面を変更した場合は原本18章の該当項目を実画面で確認する。

## Tailwindの対応表

既存デザインとTailwind標準パレットを引き継がず、テーマをリセットして仕様値を定義している。色名・キーはドットとcamelCaseをkebab-caseへ変換する。

| JSON / 用途                            | Tailwind / CSS                                              |
| -------------------------------------- | ----------------------------------------------------------- |
| `background.app`                       | `bg-background-app`                                         |
| `text.heading` / `text.body`           | `text-text-heading` / `text-text-body`                      |
| `border.control` / `border.subtle`     | `border-border-control` / `border-border-subtle`            |
| `row.selectedHover`                    | `hover:bg-row-selected-hover`                               |
| `dangerButton.foreground`              | `text-danger-button-foreground`                             |
| `shadow.floating` / `.modal` / `.drag` | `shadow-floating` / `shadow-modal` / `shadow-drag`          |
| `typography.pageTitle`                 | `text-page-title`（サイズ・行高・weightを含む）             |
| `radius.control` / `.dialog`           | `rounded-control` / `rounded-dialog`                        |
| `spacing` 4〜64                        | `1, 2, 3, 4, 5, 6, 8, 10, 12, 16`。例: `p-8` = 32px         |
| `layout.contentMaxWidth.Home`          | `max-w-home`。News、Settingsも同様                          |
| `layout.sidebarWidth`                  | `w-(--mona-layout-sidebar-width)`                           |
| `controls.minHeight`                   | `min-h-(--mona-controls-min-height)`                        |
| `controls.buttonPaddingInline`         | `px-(--mona-controls-button-padding-inline)`                |
| `motion.hover` / `.modal`              | `duration-hover ease-hover` / `duration-modal ease-modal`   |
| `motion.groupCollapse`                 | `duration-group-collapse`                                   |
| `layout.narrowMode.viewportWidthBelow` | `shell:` = 900px以上。基本スタイルを900px未満用にする       |
| Gallery利用可能幅                      | `@container` と `@gallery-2:` 〜 `@gallery-5:`              |
| `log`                                  | `design-log`、`text-log-warning`、`text-log-error`          |
| `imageCaption`                         | `bg-image-caption-background text-image-caption-foreground` |

色・影は全Light/Dark値を `--mona-*` に保持し、`@theme inline` から参照する。レイアウト・Controlの数値は階層を保った `--mona-layout-*` / `--mona-controls-*`、Motionは `--mona-motion-*` で参照できる。例: `--mona-layout-instance-modal-footer-min-height`。原本の参照クライアント最小寸法はテスト条件であり、rootのmin-widthに使わない。Instances/Galleryの最大幅nullは上限なし。列数は単位なし、viewport閾値はpx、寸法・文字は16px基準のrem、行高は相対値、時間はmsへ変換する。

`originalSemanticReferences` は色相の由来であり実装用パレットではない。本文の説明、Reduced motion方針、OSの選択色などをCSSの値として機械変換しない。JSONの全記述がutilityになるわけではない。

## Themeとフォント

`<html>` の `data-theme` が未指定または `system` ならOSに追従する。`light` / `dark` はOSより優先される。設定画面で保存された選択をこの属性に反映し、初期値はSystemとする。属性変更にはReactの再mountは不要。通常の色を `dark:` で二重指定せず、同じ意味のutilityを両Themeで使う。

Geist Variable、Noto Sans JP Variable、Geist Mono VariableをFontsourceからインストールし、Viteがフォントファイルを同梱する。配布ライセンスは `public/fonts/licenses/` に保持し、ビルド成果物にもコピーされる。起動時にGoogle Fonts等へ接続しない。CSS内のFamily名はパッケージの登録名に合わせている。日本語Monoの `Noto Sans Mono CJK JP` Regularも `public/fonts/` に同梱し、OFLライセンスを保持している。[公式配布元](https://github.com/notofonts/noto-cjk/blob/main/Sans/Mono/NotoSansMonoCJKjp-Regular.otf)のフォントを使用する。フォント本体は約16MBで、起動時の外部配信には依存しない。Windowsでの実際の字形・行高は引き続き実機で確認する。

技術値・Version・ID・Logは `font-mono`。日時や見出しはSans。数値列や経過時間は `tabular-nums`。通常UIのLabelは必要箇所だけ `select-none` にし、名前・説明・記事・Path・Address・Log・エラー詳細は選択可能に保つ。

## 基本の実装例

```tsx
<main className="design-surface min-w-0 p-4 shell:p-8">
  <h1 className="text-page-title text-text-heading">Instances</h1>
  <button
    type="button"
    className="design-focus mt-6 inline-flex min-h-(--mona-controls-min-height) items-center justify-center gap-2 rounded-control bg-primary-background px-(--mona-controls-button-padding-inline) text-button text-primary-foreground transition-colors duration-hover ease-hover hover:bg-primary-hover active:bg-primary-pressed disabled:bg-disabled-background disabled:text-disabled-foreground"
  >
    Play
  </button>
</main>
```

`design-surface` は画面/Portalのフォント・通常色の起点。`src/App.css` はトークンを使った共通コンポーネント定義で、`components` layerに配置する。旧CSSは削除済み。新しい表示はsemantic utilityと共通部品を組み合わせ、色や寸法を独自に再定義しない。

Inputは `bg-control-background border border-border-control rounded-control text-body placeholder:text-control-placeholder design-focus` と最小高さを使う。通常のSeparatorは `border-border-subtle`。通常ボタン・PanelにShadowを付けない。Dangerの最終確認は `bg-danger-button-background text-danger-button-foreground`、Force Quitは `text-danger-foreground border-danger-foreground`、一般Delete導線はNeutralを使い分ける。

寸法は高さ固定よりmin-heightを優先し、文字拡大と折返しを許可する。IconはLucide、currentColor、stroke 2。Icon-only buttonにはAccessible nameと36pxの操作領域、Compact Table内でも32pxを確保する。

Galleryは親に `@container`、子に次のクラスを付ける。Window幅基準の `shell:` を列数に使わない。

```text
grid grid-cols-1 gap-4 @gallery-2:grid-cols-2 @gallery-3:grid-cols-3 @gallery-4:grid-cols-4 @gallery-5:grid-cols-5
```

列の閾値は456 / 692 / 928 / 1164px相当。画像は `aspect-gallery object-cover rounded-image`、Lightboxは元の比率でcontainする。Captionは黒72%の面で文字全体を覆い、HoverとFocusの両方で表示する。

## 画面と挙動のチェックポイント

| 対象           | 実装で守ること / 原本                                                                                 |
| -------------- | ----------------------------------------------------------------------------------------------------- |
| Shell          | Sidebar240px、Main左揃え32px、900px未満はDrawerと16px。標準OSタイトルバー。4章                        |
| Home           | Now Playing → Recent Instances → Screenshots → News。Heroや統計Cardを追加しない。6章                  |
| Instances      | Row64px以上、Icon40px、Action常設、Separator。Group内Sort、Group間移動にはMenu代替を用意。7章         |
| Instance Modal | 周囲32px/狭幅16px、最大幅1200px、Header/Tabs/Footer固定、本文だけScroll。全11Tabへキーボード到達。8章 |
| Modal / 編集   | Escは1イベント1レイヤー、Focusを閉じ込めて戻す。未保存Draftの保存/破棄ガード。9〜10章                 |
| 処理           | Modal FooterとSidebarは同じ実状態を参照。閉じても処理取消にしない。架空の%や成功を表示しない。8・13章 |
| Settings       | Select/Switchは保存結果を反映、入力はDraftとApply。失敗時Draftを残す。10章                            |
| Log            | 常時Dark、選択・過去行閲覧を維持、秘密値を除外。`design-log` 内の `design-focus` は白Outline。11章    |
| 削除           | 対象と影響を示す確認。Instanceは名前一致、実行中削除不可。Force QuitはCancel初期Focus。8・16章        |
| Motion         | Hover150ms、Menu180ms、Modal220ms/8px。SpinnerもReduced motionで止め、進捗文字は残す。17章            |

背景Gradient・Glass・Hero・全面Card・Command Paletteは作らない。Skeleton専用Shimmerだけは例外で、4秒で静止するロジックを実装する。Reduced motionのCSSはanimation/transition/smooth scrollを停止する。JS/WAAPIで動かす場合も設定を読み、最終状態を即表示する。静止したレイアウト用transformを一律削除してDialogの位置を壊さない。

Focusは `design-focus` で2px Outline＋2px Offset。親overflowで切らない。High contrastではシステム色を尊重し、スクロールバーを隠さない。文字色のコントラストと状態ラベルの意味を保ち、BusyやReadonlyにDisabled色を使わない。

## 検証の到達範囲

`pnpm check` は設定・コンパイルの確認。実画面のアクセシビリティや操作適合を保証しない。UI移行時はLight/Dark、1024×640・1440×900、200%拡大・実効幅320px、長い日本語・Path、空/未取得/Error、キーボード、Reduced motion、High contrastを原本18章に従って確認する。

Tailwindの設定方式は [公式Themeドキュメント](https://tailwindcss.com/docs/theme)、Vite連携は [公式導入手順](https://tailwindcss.com/docs/installation/using-vite) を参照。

## フロントエンドの検証（2026-09-09）

- `pnpm check`: トークン同期、utility生成、書式、lint、型、production build。
- `pnpm test:ui`: Playwright / Chromium。テスト専用のTauri IPCモックで、実際のReact画面を操作する。モックを本番アプリへ組み込まない。
- 画面: 1440×900、1024×640、実効幅320px、文字200%、Light/Dark保存、OS High contrast、Reduced motion、狭幅Drawer、11個のTabのキーボード到達、200件の一覧と検索。
- 操作: Draftの保存失敗と維持、Escで1レイヤーだけ閉じる、作成後Overview、Mod検索への到達、名前一致による削除、強制終了失敗のRunning保持、終了通知までStopping維持、ログの資格情報マスキング。
- 未検証: Windows/AppContainerでの実ゲーム起動、実Microsoft認証、実Modダウンロード、スクリーンリーダー。IPCモックの成功をこれらの実機検証の代わりにしない。

初回のブラウザーテスト前に `pnpm exec playwright install chromium` を実行する。実データを持たない通常ブラウザーではプレビュー説明とEmpty stateを表示し、ゲーム操作を有効化しない。

コンポーネントの責務と配置は [フロントエンドアーキテクチャ](../docs/frontend-architecture.md) に従う。機能固有の部品は `src/features/` 内に配置する。

## Permissionsタブ（2026-09-12）

Instance ModalにPermissionsを追加。ゲームデータ書き込み・ナレーターのSwitchは保存結果を反映し、次回起動の共通ポリシーに接続する。保存失敗時は確定値と再試行を残し、保存中は重複操作を防ぐ。固定権限は表示のみ。原本8.2・18章のTab数を更新し、寸法・色は既存トークンを再利用したためJSONの値は変更していない。

Playwrightでは11タブへのキーボード到達、インスタンス別の保存・失敗と再試行・保存中のタブ移動/閉じる操作・実行中と未対応OSの編集禁止を確認。PermissionsのLight/Dark、1440×900・1024×640・実効幅320px、200%文字とHigh contrast/Reduced motionも確認した。IPCモックの保存テストと、Rustの実ファイル保存/ポリシー変換テストを区別する。

Instanceの表示名・Permissionsの保存中・完了は、アプリ画面全体の右下のBase UI Toastへ統一する。「保存中…」は自動消去せず、同じ通知を成功・失敗へ更新する。途中で通知を閉じても処理は継続し、結果を再表示する。本文・Footer・Applyボタンの保存中表示は置かない。成功通知は5秒、Hover/Focusで停止し、保存成功は最大3件を重ねて表示する。保存失敗と再試行は本文に残す。

Toastは共通managerの `notify({ title, type })` で表示する。typeはneutral（既定）/success/error/warning。通常はニュートラル、成功は緑、失敗は赤、警告は専用のオレンジトークンを使う。縦padding4px、閉じる操作32pxの小型表示とし、枠線はsubtleまたは意味色20%。画面右下16pxに固定し、ModalのFocus境界内で操作可能にする。失敗通知は自動消去せず、本文のエラーと再試行を残す。

ToastのstackはBase UIのindex・height・offsetとexpanded/behind状態を使用する。通常は背後の本文を隠し、Hover/Focusで展開する。出入り・並び替え・スワイプ退出をアニメーション化し、Reduced motionではtransitionを停止する。幅280px・motion500msはトークンから生成する。

LinuxのPermissionsも同じ保存APIとSwitchを使用する。Wayland優先と、その場合はX11接続を公開しないことを明記する。X11/XWayland・PulseAudio互換サーバーの接続が、他クライアント操作・録音を含む互換許可であることも固定権限の下に明記する。

## Permissionsの細分化（2026-09-12）

ファイルの全体設定と7つの固定フォルダー、通信、音声、マイク、クリップボード、ナレーターを同じ保存APIで扱う。項目定義・依存条件はfeature内に置き、既存Base UI SwitchとButtonを使う。OS非対応項目には状態説明を表示し、操作可能なSwitchを置かない。別OSの非対応設定が残った場合は明示的な解除ボタンを表示する。全体書き込みOFFは個別設定を保持したまま実効アクセスを読み取り専用にする。トークンの変更はない。

キャッシュとOS連携に「スキンキャッシュ」「日本語入力・全画面連携」「描画キャッシュ」を追加。3項目は旧JSONでも既定ON。キャッシュはゲームの書き込みスイッチに連動させず、未対応のOFF設定の持ち込みには既存の明示修復導線を使う。
