# MonaLauncher Web Design Definition v1.1

**版:** 1.1  
**作成日:** 2026-09-09  
**位置付け:** v1を置き換える、統合されたデザイン・操作仕様。差分資料ではない。  
**成果物:** 本文書、`MonaLauncher_Design_Tokens_v1.1.json`。  
**検証範囲:** 選定した配色ペアの数値検査と仕様の整合性確認。実アプリの実装・画面描画・操作試験は未実施。

## 0. 変更の目的と優先順位

白黒を中心にしたVercel系のミニマルUI、標準密度、左Sidebar、Row中心の一覧、大型Instance Modalを維持する。MinecraftらしさはUI装飾ではなく、ユーザーが設定するアイコン、World画像、Server favicon、Screenshotなど実コンテンツに限定する。

改善の目的は「装飾を増やす」ことではなく、「状態と操作を見失わない」ことである。判断の優先順位は、データ保護 → 操作の到達可能性と可読性 → 状態の一貫性 → 視覚的な簡潔さ → 微細な演出、とする。

特に修正した点は、用途別のコントラスト、Modal内の進捗、編集の保存と閉じ方、タブの到達性、Galleryの文字可読性、DragとSortの意味、最小幅・拡大表示への対応である。

「カード禁止」ではなく「通常の情報整理をカードに依存しない」とする。フォームの枠、独立した警告、Floating UI、Drag previewは、その役割がある場合に限って囲える。ボタンや通常PanelにShadowを追加しない。

本文書内の寸法は100%表示時のCSS px基準値。文字・操作領域の拡大に追従させる。固定pxの高さで文字を切り落とさない。JSONはプロジェクト固有の形式であり、特定のトークン交換規格への準拠は主張しない。

---

## 1. 変えない視覚的な骨格

| 項目 | 決定 |
|---|---|
| 全体 | Vercel系・無彩色中心・少し無機質・標準密度 |
| 個性 | 色や装飾でなく、小さい操作フィードバック |
| OSタイトルバー | 標準のものを使用 |
| Sidebar | 240px。ロゴ・アプリ名・アカウント常設領域なし |
| Main | 左揃え。原則32pxの内側余白 |
| 一覧 | Row＋Separator。Instanceは標準64px・アイコン40px |
| Font | 英数字Geist、日本語Noto Sans JP。技術値・LogはMono |
| Page title | 24px / 32px / 600 |
| Control | 標準36px。角丸10px |
| Icons | Lucide、標準stroke 2px、`currentColor` |
| Dialog | 角丸14px。1pxの境界線 |
| Play | 通常サイズのモノクロPrimary。巨大化しない |
| Theme | Systemが初期値。Light / Dark。設定内のみで切替 |
| Command Palette | 作らない |
| 装飾 | Hero、Glass、背景Gradient、全面Card配置を作らない |

実装ライブラリ自体を禁止しない。利用する場合は、その既定の色・角丸・Shadowではなく本仕様のトークンを適用する。

## 2. 配色：用途を分け、Light / Darkを個別に定義する

### 2.1 基本トークン

| Token | Light | Dark |
|---|---|---|
| `background.app` | `#FFFFFF` | `#000000` |
| `background.sidebar` | `#FAFAFA` | `#0A0A0A` |
| `background.floating` | `#FFFFFF` | `#111111` |
| `background.subtle` | `#F5F5F5` | `#171717` |
| `text.heading` | `#000000` | `#EDEDED` |
| `text.body` | `#171717` | `#EDEDED` |
| `text.secondary` | `#666666` | `#A1A1A1` |
| `text.navigation` | `#525252` | `#A1A1A1` |
| `border.subtle` | `#E5E5E5` | `#262626` |
| `border.control` | `#8A8A8A` | `#737373` |
| `border.controlHover` | `#737373` | `#A1A1A1` |
| `control.background` | `#FFFFFF` | `#000000` |
| `control.placeholder` | `#666666` | `#A1A1A1` |
| `navigation.hover` | `#F5F5F5` | `#111111` |
| `navigation.selected` | `#F0F0F0` | `#171717` |
| `row.hover` | `#FAFAFA` | `#111111` |
| `row.selected` | `#F5F5F5` | `#171717` |
| `row.selectedHover` | `#F0F0F0` | `#202020` |
| `disabled.background` | `#E5E5E5` | `#262626` |
| `disabled.foreground` | `#B3B3B3` | `#737373` |
| `focus.ring` | `#000000` | `#FFFFFF` |

`surface`、`selected`のような用途が曖昧な名前だけで部品を組み立てない。Inputは常に`control.background`を参照し、Floating UIの面色と混同しない。

`border.subtle`はSidebar境界、Separator、Dialog輪郭などの構造線に用いる。空のInput、未チェックCheckbox、SwitchのOFF輪郭など、境界が識別の主要な手掛かりとなる箇所には`border.control`を使う。すべての線を濃くする変更ではない。[S2]

非活性Control専用のDisabled色は維持する。ただし処理中の状況説明・Loadingラベル・読取専用値にDisabled色を流用しない。そこには通常文字色を使う。

### 2.2 状態色

通常の操作、選択、リンク、Focusはモノクロ。色を使うのは成功・実行中・警告・エラー・更新情報などの意味がある表示だけ。

| 状態 | Light文字 | Dark文字 | Light補助面 | Dark補助面 |
|---|---|---|---|---|
| success | `#166534` | `#4ADE80` | `#F0FDF4` | `#0C2012` |
| danger | `#C32932` | `#F87171` | `#FFF1F2` | `#2B1114` |
| warning | `#92400E` | `#FBBF24` | `#FFFBEB` | `#251A08` |
| info | `#1D4ED8` | `#60A5FA` | `#EFF6FF` | `#0B1930` |

元の色相基準（Green `#16A34A`、Red `#E5484D`、Amber `#D97706`、Blue `#2563EB`）は維持し、文字として使う色は濃淡を補正する。ドットなどの`*.mark`はJSONで別定義する。

色だけに意味を持たせない。Running、Offline、Update available、Errorなどの文字を必ず伴わせる。Off/未取得/Disabledは原則Neutral。実行中の緑を点滅・Pulseさせない。

状態の薄い背景面は、文脈から独立した警告やフォーム内エラーなどに限定する。通常の一覧の状態文字すべてをBadgeにしない。

本文相当の文字について4.5:1以上、Controlの識別に必要な輪郭・状態表示について3:1以上を選定基準とする。装飾的な区切り線や非活性Controlまで一律に同じ基準を課すものではない。全画面・全状態の適合性は実装後に別途検査する。[S1][S2]

### 2.3 Primary / Secondary / Danger

| 種類 | Light | Dark |
|---|---|---|
| Primary | 黒面・白文字。Hover `#1A1A1A` | 白面・黒文字。Hover `#E5E5E5` |
| Secondary | 白面・`border.subtle`・本文色 | 黒面・`border.subtle`・本文色 |
| Ghost / Icon button | 透明面。Hoverのみ薄い背景 | 同じ構造 |
| Force Quit | `danger.foreground`の文字・輪郭 | 同じ役割のDark色 |
| 破壊操作の最終確認 | `#C32932`面・白文字 | `#F87171`面・黒文字 |

赤い確認ボタンをDarkでも白文字のままにしない。確定前の一般的なDelete導線はNeutral、MenuのDeleteは赤い文字、Force Quitは赤いOutlineという区別を維持する。

---

## 3. Typography、寸法、描画

### 3.1 Fontと文章

英数字の通常UIはGeist Sans、日本語はNoto Sans JPを使用する。技術値とLogはGeist Monoを優先し、日本語のMonoフォールバックとしてNoto Sans Mono CJK JPを指定する。Fontは製品に同梱する方針とし、起動時の外部Font配信への接続を前提にしない。字体の位置合わせは同梱版で固定する。[S10][S11]

| 用途 | Size / Line-height / Weight |
|---|---|
| Page / Instance Modal title | 24 / 32 / 600 |
| Section / 通常Dialog title | 18 / 26 / 600 |
| Body・設定説明 | 14 / 21 / 400 |
| Navigation / Group | 14 / 20 / 500 |
| Button | 14 / 20 / 500 |
| Small | 13 / 18 / 400 |
| Caption | 12 / 16 / 400 |
| Log | 13 / 20 / 400 |
| News記事本文 | 16 / 28 / 400 |

見出しを過度に太くしない。ALL CAPSは使わない。長文の記事本文に一覧用14pxをそのまま流用しない。日本語を無理に詰める負のletter-spacingは使わない。

通常のUIラベルは選択不可とする一方、ユーザーが付けた名前、説明文、記事、Version、Path、Address、Log、Error detailsは選択可能とする。画面全体に`user-select: none`を適用しない。

バージョンやIDはMono。日時・件数・画面タイトルまでMonoにしない。数値列・経過時間にはtabular numeralsを使い、数字の更新による横幅の変動を抑える。

### 3.2 Spacing / Radius

Spacingは`4 / 8 / 12 / 16 / 20 / 24 / 32 / 40 / 48 / 64`。

Page paddingは32px、Page titleからToolbar/本文まで24px、Toolbarから一覧まで16px、Section間32px。フォームのLabelと説明は4px、説明から下置きControlまで8px、設定項目の上下paddingは16px。

RadiusはControl・Image 10px、Menu 12px、すべてのDialog 14px、Checkbox 6pxに確定する。半端な5〜6pxという範囲は残さない。

### 3.3 Border / Shadow

通常線は1px。Tab active underline、Focus ring、Progressの基準高は2px。厚さ2pxのProgressに小さいハンドルやクリック操作を付けない。

ShadowはFloating UIとDrag previewだけに用いる。Lightの基準はFloating `0 2px 8px rgba(0,0,0,.08)`、Modal `0 16px 48px rgba(0,0,0,.18), 0 4px 12px rgba(0,0,0,.08)`。Darkの具体値はJSONに定義する。Blurは使わない。

## 4. Application shell

### 4.1 ウィンドウと幅

標準表示の最小設計対象は、**OSタイトルバーを除くクライアント領域1024×640**。実装ではOS枠の外寸と混同しない。利用可能な作業領域がこれより小さい場合に、ウィンドウを画面外へ強制することは禁止する。

Mainの最大幅はHome 1200px、Instancesなし、News 1000px、Galleryなし、Settings 960px。いずれも左揃え。

Sidebar240px・Page padding32pxを使った最小幅ではMainの内側は概ね720px。この幅を標準の下限テストにする。全ページの余白を削って詰め込むのではなく、メタデータの省略と項目内の折返しで対応する。

### 4.2 Sidebar

上部NavigationはHome / Instances / News / Gallery。Sidebarの外側insetは12px、項目内padding-inlineも12px、各項目36px以上。項目間4px、IconとLabel間8px。Navigation iconは18px、stroke2px。

Selectedは薄い背景＋濃い文字。左indicatorは追加しない。SidebarとMainの間は`border.subtle`。

下部は、Activity → 24pxのGroup間隔 → Separator → Settings。Activity内はDownload概要、その下にNow PlayingまたはLast Played。通常は背景なし、Hover / Focus-within時に薄い背景を表示する。

Navigation領域は独立して縦Scroll、Activity / Settingsは下部固定。Activityは一覧を積み上げず、Download全体で1つの概要、起動状況で1つの概要に集約する。複数件ある場合は件数付きPopoverに展開する。履歴も起動中のInstanceもなければLast Played欄を表示しない。

Now Playingは最後に起動した実行中Instanceを代表表示し、複数実行中なら「ほかN件」を同じ概要に表示する。実際に複数起動に対応していない実装では、その機能をUIだけ作らない。同じInstanceの重複起動は防止する。

### 4.3 拡大表示の例外

通常の1024px以上・100%表示では240px Sidebar、32px padding、Page titleとActionの横並びを守る。

実効CSS viewportが900px未満になる拡大表示・小さい作業領域では、SidebarをMenu buttonから開くDrawerに切り替え、Page paddingを16pxにする。Header/Toolbar/Modal footerは必要な場合に折り返せる。Drawerの中にも同じNavigation・Activity・Settingsを残す。これは通常画面のモバイル化ではなく、操作を画面外へ追い出さないための例外。

本文と設定フォームは縦にReflowする。Galleryは1列まで減らせる。大きなTable・Logなど二次元構造が必要な領域だけ局所的な横Scrollを許可する。画面全体に固定`min-width:1024px`を設定しない。文字200%拡大と実効幅320px相当を受入テストに含める。[S12]

### 4.4 Scrollbar

OS/browserのScroll設定を尊重する。Overlay Scrollbarがある環境では自然な自動非表示に従い、常時表示を選んだOSでは隠さない。独自の`scrollbar-width:none`などでScrollの手掛かりを消さない。[S8]

## 5. 共通Controlと操作

Button/Input/Selectの標準高は36px、Button横padding14px、Iconと文字間8px。`height`で内容を切るのではなく標準状態のmin-heightとする。

InputはLight白・Dark黒、`border.control`、Inset shadowなし。PlaceholderをLabelの代わりにしない。Searchには検索対象を明示した可視または支援技術向けLabelを付ける。SelectのChevronDownは常時表示する。

Icon-only buttonは原則36px角。CompactなTable内でのみ32px角を許可する。Tooltipに頼らずAccessible nameを設定する。Copy操作は対象のデータを明示した名前にする。

Checkboxは見た目16px・角丸6px、操作領域32px角。CheckedはLight黒/白check、Dark白/黒check。Switchは見た目36×20px・Thumb16px、操作領域の高さ32px以上。ONはPrimaryの面色と反転色のThumb、OFFはSubtleな面色と本文色のThumbで区別する。未チェック/OFF状態は`border.control`で輪郭を示す。[S6]

Focus-visibleは**2pxのモノクロOutline＋2px Offset**。塗りつぶしボタンでも輪郭が埋没しないようOffsetの背景をその場の面色に合わせる。Light中のDark Logでも、その局所面に合わせ白いRingを使用する。Ringを親のoverflowで切らない。Focus移動先を固定FooterやHeaderの下に隠さない。[S13]

通常行はHover背景のみ。ActionはHoverのときだけ出現させず常時表示する。Row自体はカードとして個別に囲わない。反復する項目の境界にSeparatorを置き、一般的なSection間は余白を優先する。

## 6. Home

表示順は、Now Playing（実行中のみ）→ Recent Instances → Recent Screenshots → News。すべて通常Section。Hero、利用統計Card、挨拶だけの大きな領域は作らない。

Now PlayingはInstance名、Version/Loader、実行中であること、Open Instanceを表示する。経過時間はSmall。状態dotは静止した緑。

Recent Instancesは最新3件。行はInstancesと共通。未使用Instanceを「最近使用した」と偽って並べない。履歴がなくInstanceがある場合は、最近使用した履歴がない旨とInstancesへのリンクを表示する。

Recent Screenshotsは最新3枚を標準幅で横一列。拡大時は2列・1列まで許可する。Newsは最新3件。各Sectionに必要な場合だけモノクロのView allリンクを置く。Download専用Sectionは追加しない。

初回はNow PlayingとLast Playedを表示せず、HomeにCreate instanceの短い導線を置く。アカウント未設定なら説明付きのSign in導線を同じページ内に置く。認証を必要としないローカル閲覧までブロックする全面Wizardにはしない。

## 7. InstancesとGroup

### 7.1 Header / Toolbar

Page headerは左にInstances、右にAdd instance。直下にSearch / Filter / Sort。SearchにCommand Palette風ショートカット表記は付けない。

Filterの初期範囲はMinecraft versionとMod Loader。実データから選択肢を作る。適用件数をFilterラベルに表示し、Clear filtersを用意する。SearchとFilterはGroupをまたいで適用される。

SortはLast Played（初期値）、Name、Created。Sortは各Group内に適用する。Last Playedがない行は後ろ、同順位は名前と安定IDで決める。Groupは名前順、Ungroupedは最後。並べ替えで操作対象を別Instanceへすり替えないよう、選択とFocusはIDで保持する。

### 7.2 Rowと長い文字

標準は64px、Icon40px、Iconと本文の間12px。本文は名前＋補足の2行。右側のPlayとMenuの必要幅を先に確保する。

補足はMinecraft version → Mod Loader → Mod件数 → Last Playedの順。横幅不足ではLast Played、次にMod件数を隠す。VersionとLoaderは可能な限り残す。名前は1行末尾省略し、Hover/FocusのTooltipおよびModalで全文を読めるようにする。文字を小さくして対応しない。

幅拡大・日本語の行高増加が必要な場合はRowを64pxより高くしてよい。IconやActionを圧縮しない。

名前または行の操作用領域でInstance Modalを開く。Play/Menu/Checkbox/Copyの操作をRow clickへ伝播させない。選択した文字をドラッグしただけでModalを開かない。キーボードではInstance名の明示的なリンク/ボタンから開けるようにする。

実行中の行の主要操作はOpen Instanceへ変える。Force Quitを多数の行へ露出させず、Instance Modalの固定Footerに残す。

### 7.3 Group

Group/Folderは同じ1階層の分類を意味する。任意作成・改名・折り畳みを提供するが、v1.1で多段階のフォルダー木は追加しない。見出し14px/500、件数はSmall。全体を枠で囲わない。

Drag & Dropは**Group所属の変更**。自動Sort中に行の任意位置へ移せるようには見せない。Drop先Group全体の薄い背景と見出しの2pxの枠で移動先を示し、行間Insertion lineは出さない。Drag previewのみShadowを許可する。

同じ操作を`··· → Move to group`からも提供する。これは必須の代替であり、Dragが唯一の方法にならないようにする。[S5]

Group削除は分類だけを削除し、InstanceはUngroupedへ移す。InstanceデータをGroup削除に巻き込まない。削除確認にこの挙動を明記する。

検索中は一致するGroupを一時的に展開し、検索前の折り畳み状態を保存する。検索結果0件とInstance自体0件は別のEmpty stateにする。

## 8. Instance Modal

### 8.1 外形

標準幅`min(1200px, viewport width - 64px)`、高さ`viewport height - 64px`。周囲32px、角丸14px、1pxの`border.subtle`。Lightは白面、Darkは`#111111`。BackdropはLight黒25%、Dark黒50%、Blurなし。

幅900px未満の拡大表示では周囲16px。縦サイズ不足時もCloseと操作を画面外へ出さない。

```text
┌────────────────────────────────────────────────────────────┐
│ Main Instance                                          ×   │
├────────────────────────────────────────────────────────────┤
│ Overview  Log  Version  Mods  Resource Packs  ...       ›    │
├────────────────────────────────────────────────────────────┤
│                                                            │
│                     Tab content                            │
│                     （ここだけScroll）                      │
│                                                            │
├────────────────────────────────────────────────────────────┤
│ Downloading libraries · 68%      [Open Folder] [Preparing…] │
│ ━━━━━━━━━━━━━━━━━━━━━━━                                    │
└────────────────────────────────────────────────────────────┘
```

Headerは最小64px、タイトルとCloseだけ。Version/Loader/Playを追加しない。Tabs40px、Content padding24px、Footer最小72px。Header/Tabs/Footerは固定し、ContentだけScroll。固定は画面に対する絶対座標ではなくModal内の区画として実装する。

### 8.2 Tabs

順番はOverview / Log / Version / Mods / Resource Packs / Shader Packs / Worlds / Servers / Screenshots / Permissions / Settings。上部の横並び、Activeは2px underline。ラベルをアイコンだけにしない。More menuへ格納せず、横Scrollを維持する。

Overflowがある側にだけ32px以上の左右Scroll buttonを表示する。端まで進んだ側は不要。これはタブの代替MenuではなくScroll補助である。フォーカスしたTabと選択したTabは自動的に可視範囲へ移動する。Gradient fadeは使わない。

左右矢印で即時切替。Home/Endで先頭/末尾。TabキーはTablist全体で1つの停止点とし、次にPanelへ移る。軽量な初期表示データはModal表示時に取得・キャッシュし、Panelの枠と内容は待ち時間なく切り替える。重い未取得データはPanel内のLoading表示を経由し、取得結果で既存のFocusを移動させない。キー移動ごとに描画完了を待たせない。[S4]

Modalを閉じて再度同じInstanceを開いた場合は、前回TabとScroll位置を復元する。削除された項目に戻ろうとした場合は該当Tabの一覧上部へ戻す。

Permissionsでは、このインスタンスのゲーム全体に適用する権限を「ファイル」「通信とデスクトップ」に分ける。ファイルはゲーム全体の書き込み、ワールド・スクリーンショット・リソースパック・シェーダーパック・Mod・Mod設定・ログの用途別書き込みをSwitchで変更する。全体OFF時は用途別項目を読み取り専用として無効化し、保存した個別値は保持する。通信、通常音声、マイク、クリップボード、ナレーターはバックエンドの対応状況に従う。「キャッシュとOS連携」にスキンキャッシュ、日本語入力・全画面連携、描画キャッシュを追加し、新規・既存インスタンスとも既定ONとする。キャッシュはゲーム全体の書き込み設定と独立する。日本語入力・全画面連携の個別制御はmacOSのみ、描画キャッシュの個別制御はmacOS/Linuxで対応し、その他のOSでは許可状態と個別制御未対応の説明を表示する。macOSのMetalキャッシュは同じユーザーのJavaアプリと共有される限定領域であることを説明する。Windowsの通常音声、Windows/Linuxのマイク・クリップボードの個別変更にはSwitchを置かず、実際の制約を表示する。マイクON時は通常音声OFFへ変更できず、理由を表示する。別OSから持ち込んだ未対応設定は明示的に解除・修正できる。全変更は自動保存・次回起動から適用し、既存インスタンスの従来の許可を維持して通信やマイクを自動許可しない。通信ONはすべてのModのインターネット/LAN送受信を許すことを説明する。共有コードは読み取り専用、画面/入力は固定。実行中・保存中・Mod操作中・未対応OSでは編集を禁止する。失敗時は確定値と近傍の再試行を維持する。狭幅ではSwitchを説明文の下へ移し、値・寸法は既存Switchトークンを使う。LinuxのWayland/X11とPulseAudioの互換許可についても説明する。

### 8.3 Footerと起動状態

右側はOpen Folder、その右に主要操作。通常はPlay、実行中はForce Quit。双方36pxの通常サイズ。

左側は処理があるときだけ状態文字とProgress。何もないときに「Ready」や利用統計を埋めない。SidebarとFooterは同じ処理データを参照する。Modal内で開始した処理の確認を、操作不可の背景Sidebarに依存させない。[S3]

| 状態 | Footer左 | 主要操作 | 補足 |
|---|---|---|---|
| Ready | 空 | Play | 二重起動を防ぐ |
| Authentication needed | 再認証が必要 | Sign in | 認証完了だけで勝手に起動しない |
| Preparing | 検証・準備など実際の段階 | Preparing… | Busyラベルは通常コントラスト |
| Downloading | 対象・進捗・取得できるときだけ% | Preparing… | ダウンロード詳細はFooterからも開ける |
| Starting | Starting Minecraft… | Starting… | クリックをRunningとみなさない |
| Running | Running・実測の経過時間 | Force Quit | 赤Outline・確認必須 |
| Stopping | Stopping Minecraft… | Stopping… | 重複実行不可 |
| Failed | 失敗要約とShow details/Log | Retry | 再試行可能な場合に限る |

Play押下直後に処理開始を視覚的に返す。ボタン幅はBusyラベルで不必要に跳ねないよう、通常/Busyの長い方を収容する。Progressは分母を取得できる場合のみ%を出す。不明な進捗を時間経過で水増ししない。

Download取消は、処理側が安全な取消を提供する段階だけ詳細Popoverに表示する。提供していない操作をUI上で成功したことにしない。Modalを閉じることと処理の取消を結び付けない。

### 8.4 Force Quit

最終確認は480px Dialog。対象Instance名を表示し、「保存されていないワールドの進行状況が失われる可能性があります」と明記する。初期FocusはCancel。Enter一押しで初期状態から強制終了しない。

確認しても終了完了が観測されるまではStopping。失敗時はRunningを維持してエラーを表示する。Clickした時点でNow Playingを消さない。

## 9. Modal / Popoverを閉じる規則

### 9.1 EscとBackdrop

**Esc1回につき最前面の対象を1つだけ閉じる。** Menu、Combobox、Popover、確認Dialog、Lightbox、Instance Modalの順は実際の表示レイヤーに従う。「全部まとめて閉じる」は廃止する。

Backdrop clickで閉じる方針は維持。ただしpointer-downとpointer-upの両方が同じBackdrop上にある場合だけ閉じる。Dialog内から外へドラッグして離した操作で閉じない。1回のclickを背後のModalまで伝播させない。

確認DialogのBackdrop/EscはCancel扱い。未保存の編集がある場合、親画面を閉じる前に保存・破棄のガードを適用する。

### 9.2 Focusとレイヤー

Modal表示時は背景を非操作状態にし、FocusをModal内へ移す。Tab/Shift+Tabで外へ抜けない。閲覧用の大型Modalではタイトルを初期Focus位置にできる。Closeは常時到達可能にする。閉じたら元の起点へFocusを戻し、起点が消えていたら同じ一覧内の論理的に近い場所へ戻す。[S3]

Modal内から開くMenu/Popoverは、そのModalの操作可能なレイヤーに属させる。大きいz-indexだけで背景のinert状態を回避しない。Lightboxは現在の操作領域の上に1つだけ開く。重ねたBackdropで必要以上に画面を黒くしない。

Backlogの通知を前面へ割り込ませない。確認中に別InstanceのModalを勝手に開かない。

## 10. Settingsと保存モデル

### 10.1 Global Settings

上部TabsはGeneral / Minecraft / Java / Advanced。40px underline。max-width960px。表示名は選択言語へ翻訳する。

General内にAppearance、Language、AccountsをSectionとして置く。SidebarにアカウントやTheme buttonは追加しない。アカウント管理は認証済み/期限切れ/未設定を明示する。複数アカウントは実際に対応する場合のみ選択肢を出す。

各設定はTitle＋説明を常時表示。短いSelect/Switchは右、Path/長いInput/複合Controlは下。Control幅はSelect原則200px、Path等は最大560pxか利用可能幅まで。本文を押し潰す場合は下へ移す。

Section heading18px/600、設定項目間に必要なSeparator。外周Cardは作らない。

### 10.2 保存の単位

| 種類 | 決定 |
|---|---|
| Theme、単純なSelect/Switch | 変更時に保存。処理中は同じ項目の重複操作を防止 |
| Name、Path、Memory、引数など入力途中がある値 | 編集Draftを保持。項目のApplyで確定 |
| 複数の値が一体となる設定 | Section単位のApply / Cancel |
| Version更新、削除等の作業 | 一般設定の自動保存とは分け、明示的な実行確認 |

Inputのblurだけで保存しない。単一行のEnterはApplyと同じにできるが、日本語IME変換確定中は実行しない。Apply/Cancelは編集中だけその項目の下に現れる。ゲーム起動用FooterにSaveボタンを混ぜない。

Instanceの表示名・Permissionsは、保存開始時にアプリ画面全体の右下のToastで「保存中…」を表示し、同じ通知を成功・失敗へ切り替える。保存中は自動消去せず、本文・Footer・Applyボタンに保存中表示を重複させない。通知を手動で閉じても保存は継続し、結果は再表示する。成功Toastは5秒で自動終了し、Hover/Focus中は停止する。保存成功を最大3件まで重ね、閉じるボタンを用意する。それ以外は完了時に同じ場所で短くSavedを表示する。失敗時はDraftを残し、原因とRetryを近傍へ表示する。保存が失敗した値を保存済みとして扱わない。切替型Controlの保存失敗は最後の確定値へ戻し、失敗と再試行の導線を表示する。

### 10.3 未保存の変更

未保存Draftがある状態でTab移動、Page移動、Modal close、別Instanceへの切替を行うと、共通の未保存変更Dialogを表示する。

選択肢は「編集を続ける」「破棄して移動」「保存して移動」。初期Focusは編集を続ける。保存失敗時は移動しない。確認を閉じた場合も元の編集を残す。無効な値があれば保存を許可せず、理由を示す。

保存済みの非同期処理はModalを閉じても継続する。未送信の入力と、既に始まった処理を区別する。

Global SettingsとInstance Settingsは「既定値」「このInstanceで上書き」を明示する。技術設定の変更には、即時適用か次回起動からかを説明文に含める。実際に存在しないサンドボックス権限や分離能力を設定項目だけで示さない。

## 11. Instanceの各Tab

### Overview

基本情報（Minecraft/Loader、Last Played、Game directory）と件数（Mods、Worlds、Screenshots）。Definition list/Rowで配置し、KPI Cardにはしない。件数が不明なら「未取得」と表示し、0件と区別する。CopyはPath等の詳細値の横に常時置く。

### Version

MinecraftとMod LoaderはComponent / VersionのTable。Librariesは別Section。ゲームに導入するModをLoaderや基盤Libraryの例として混在させない。値はMono。更新操作は対象・変更前後・影響を明示して確認し、クリック時に即置換しない。

### Mods

Compact Tableの標準行高は44px以上。小さいMuted headerは13px、コントラストを下げすぎない。列はEnabled / Name / Version / Status / Menu。EnabledのCheckboxは有効・無効の変更であり、行の選択ではない。

一括選択を同じCheckboxに兼ねさせない。v1.1では一括選択用の追加列やSelection modeを増やさない。Enabled/DisabledはNeutral、UpdateはInfo、WarningはAmber、ErrorはRedの文字表示。

起動中のファイル変更は既定では禁止し、その理由を一覧近傍に常時表示する。ファイルの置換・削除・Enable変更を実行中のゲームへ強引に適用しない。安全な変更予約を実装していない段階では「次回起動時に自動反映」と表示しない。

変更中は対象行の状態を示し、失敗は同じ行に表示する。Table全体をSkeletonへ戻さない。

### Resource Packs / Shader Packs

Modsと同じCompact listとToolbarを再利用する。ただしランチャーから制御できる操作だけを表示する。「導入済み」と「ゲームで選択中」を区別し、実際にゲーム設定を変更していないのにチェックだけで有効化済みと表示しない。

### Worlds / Servers

小さい実画像付きのRow。World/Server名を優先し、補足は2行目。ServerのOnline/Offline/Checking/Unknownを区別する。未取得のオンライン人数を0として表示しない。Worldの最終利用日時も確実に取得できる値だけを使う。

ServersのAddressは詳細で選択・Copyできる。Worldの削除はデータ削除確認に分離する。単なる行clickで削除や接続は行わない。

### Screenshots

Global Galleryと同じGrid・Lightboxを再利用し、対象Instanceで絞った状態にする。違うCardデザインを増やさない。

### Log

Themeに関係なく背景`#0A0A0A`、通常文字`#EDEDED`、DEBUG `#A1A1A1`、WARN `#FBBF24`、ERROR `#F87171`。13px/20pxのMono。Level名を残し、色だけで分類しない。プログラミング用合字は無効にする。

Search、Level filter、Copy/Export、Followを上部に置く。末尾を追っているときだけ自動Scroll。ユーザーが過去の行へ移動したらFollowを停止し、「最新へ移動」で再開する。Log更新で選択中テキストやScroll位置を壊さない。

長いLogは局所的な横Scrollを標準とし、Wrap toggleを提供する。Log全文を毎回Live regionで読み上げない。認証トークン等の秘密値は表示・Copy・Export前に除外する設計要件とする。Logの独立ThemeはFocusにも適用する。

## 12. Create Instance

480px Dialog、padding24px、Title18px/26px、Actions右寄せ。

Name → Minecraft version → Mod Loaderの一画面。Nameを推定で補う場合も編集可能にする。VersionはSearch可能ComboboxにRelease / Snapshot filterを付ける。初期はRelease。

Latest releaseは表示上の補助ラベルに留め、作成確定時には具体的なVersion IDへ解決して見せる。後から意図なく追従する可変Versionとして扱わない。

Mod Loaderの初期値はNone。選択したMinecraftと組み合わせられるものだけを候補にし、取得中/不明/非対応を区別する。作成失敗でName等の入力を消さない。

Create押下後はCreating…＋Spinner。作成完了後は新InstanceのOverviewを開く。Downloadが残る場合は同じInstanceの処理としてFooter/Sidebarに継続表示する。画面表示の成功だけでファイル作成成功を偽らない。

## 13. Download / Loading / Toast / Errors

### Download

Sidebar下部Groupの最上段に概要。Clickで幅320px・最大高360pxのPopoverを表示。画面端では利用可能幅に収める。対象Instance、現在段階、ファイル群、取得可能なら速度と残量を表示。縦長の一覧をSidebarそのものに展開しない。

複数Downloadの総%は意味のある共通分母が得られる場合のみ計算する。得られなければ「3件を処理中」のように件数で表す。処理完了は状態変化で示し、バックグラウンドで完了した重要作業だけ通知を出す。

### Loading

ButtonはSpinner＋ラベル、Page/List初回読込は実形状に近いSkeleton、DownloadはProgress。既存データの再取得では前の内容を保持し、更新中の小さい表示を添える。毎回画面を空にしない。

SkeletonはNeutralなShimmer。周期1400ms、連続した演出は最大4秒で静止する。長い待機を装飾で隠さず、状態文字を添える。ShimmerのGradientはLoading専用であり、通常のCard背景等へ流用しない。

### Toast

通常時は新しい通知を手前にして最大3件をstackする。幅280px（狭幅では画面幅−32px）、背後は8pxずつ見せて0.9/0.8倍へ縮小する。Hover/Focusで自然な高さと8px間隔へ展開する。出現・退出・並び替えは500msのease-out、Contentの透過と高さは150ms。スワイプ方向へ退出でき、Reduced motionでは位置関係を保ったままアニメーションを停止する。

右下、最大3件。通常成功通知を大量に出さない。コピー完了・設定保存は原則操作した場所で伝える。Instanceの表示名・Permissionsの保存成功は画面全体の右下のToastで伝える。失敗をToastだけに置き、消えたら原因が分からなくなる設計にしない。

Toastは画面右下16pxへ固定する。Modalが開いていても表示位置は画面を基準とし、DOM上は前面の操作可能な領域内に置いてFocusを維持する。通常はニュートラル、成功は緑、失敗は赤、警告はオレンジ。薄い枠線と淡い背景を使い、文字13px、縦padding4px、横左12px/右4px、閉じる操作32pxで小さくまとめる。色だけで意味を伝えず、本文と種別のアイコンを併用する。Hover/Focus中は自動終了を止める。重要エラーは自動消去しない。

### Error

項目のエラーはInline、Page取得失敗はEmpty stateと同じ上部寄りの構造。エラー文は「何が起きたか」「何ができるか」を短く書く。大きなError illustrationは使わない。

Technical detailsはShow detailsで展開。選択・CopyできるMono領域。重大な起動失敗ではLogタブも参照できる。Copy/詳細の閲覧に失敗の再現操作を要求しない。

## 14. Gallery / Lightbox

### Gallery

SearchとAll instances filter、16:9のUniform Grid、Gap16px。列数は**Window幅ではなくGridの利用可能幅**で決める。

| 利用可能幅 | 列数 |
|---|---:|
| 456px未満 | 1 |
| 456〜691px | 2 |
| 692〜927px | 3 |
| 928〜1163px | 4 |
| 1164px以上 | 5 |

各Imageの目標最小幅220px、最大5列。標準最小WindowのMain内側約720pxでは3列。Modal内や拡大時は同じルールで自然に列数を変える。

一覧は元画像を中央基準で16:9にCrop、Radius10px。原本ファイルは変更しない。通常時は画像だけで、常設のCaption Cardを作らない。

Hover/Focus-within時だけ、Instance名・日時・Menuを表示する。画像全体は暗くしないが、**文字の直下だけ黒72%の小さい背景面**を敷く。文字は白、padding4px 8px、radius6px。背景のGradientは使わない。最大2行で、画像内に収める。

Image clickはLightbox、Menu clickはMenuだけを開く。Accessible nameには対象と日時を含める。Hoverが存在しない環境では最低限のCaption/Menuを常時表示する。Hoverで出した情報はキーボードでも同等に得られるようにする。[S7]

### Lightbox

Imageを元の縦横比のままContainで大きく表示。下部にInstance名と撮影日時。画像そのものを切り抜かない。Previous/Next、Close、必要なMenuを提供し、通常画面を装飾する大きな矢印は置かない。

左右矢印で画像移動、Esc/BackdropでLightboxだけ閉じる。元のThumbnailへFocusを戻す。削除は確認を経由し、画像を開く操作と混同しない。

## 15. News

TabsはAll / Minecraft / Java Patch Notes / MonaLauncher。Allではv2ニュースとJava版パッチノートを公開日時順に統合し、Minecraftではニュース、Java Patch Notesではパッチノートだけを表示する。テキスト主体のRow、左に画像がある場合だけ160×90pxのThumbnail。画像がない記事のために装飾用Placeholderを増やさない。

記事名、短い要約、配信元、日時を表示する。記事名は標準2行まで、要約2行まで。本文と画像間16px。幅不足時はThumbnailを本文の上へ移せる。スクリーンショットのHoverメタデータ方式をNewsへ流用しない。

Mojang配信の記事は、記事名をクリックするとOSの既定ブラウザで原文を直接開く。外部リンクアイコンを添える。クリック方法の案内文は表示しない。中間のArticle Modalは表示しない。

配信JSONに含まれるタイトル・要約をプレーンテキストとして表示し、外部HTMLやスクリプトは埋め込まない。本文を推測して補完しない。ブラウザで開けなかったときは該当記事にエラーを表示し、同じ記事名から再試行できるようにする。

## 16. 認証・Offline・破壊操作

アカウントはGlobal Settings → General → Accountsで管理する。初回HomeのSign inは同じ管理先へつながる。認証切れは操作位置のInlineとFooterで示し、全画面をLogin Cardへ置き換えない。

OfflineでもローカルInstance、設定、保存済みScreenshot/Logは閲覧できる設計とする。取得済みNewsにはキャッシュ/更新日時を示す。Offline起動・権利確認の可否は実際の認証/起動基盤の判定に従う。ネットワークがないという理由だけで起動可能と推測しない。

Instance削除では名前入力による確認を必須とする。対象名はCopy可能、貼り付けも許可する。名前の入力一致を確認してからDeleteを有効にする。実行中は削除不可。削除対象ディレクトリと既知のWorld件数を示し、外部の共有Libraryやバックアップまで削除するような曖昧な説明にしない。

World単体等のデータ削除も明示的な確認を経由する。Group削除は分類の除去として別扱い。DeleteとForce Quitのように結果が異なる操作で同じ汎用「Are you sure?」を使わない。

## 17. Keyboard / Tooltip / Copy / Motion

### Keyboard

Tab/Shift+Tab、Enter、Space、Esc、Menu/Tabの矢印操作を提供する。NavigationはCmd/Ctrl+1 Home、+2 Instances、+3 News、+4 Gallery。Cmd/Ctrl+,でSettings。Modal表示中は背景NavigationのShortcutを動作させない。IME変換中はページ切替などのShortcutを実行しない。

組み込みブラウザの外部版を提供する場合、ブラウザ自身のタブ切替等のShortcutを奪わない。OS標準のClose/Zoom/文字編集操作を独自挙動へ置き換えない。

### Tooltip / Copy

TooltipはLight黒面白文字、Dark白面黒文字、radius6px、padding4px 8px、初回delay500ms。キーボードFocusでも表示し、Escで消せる。Shortcutがある操作には併記する。Tooltipだけにエラーや必須説明を隠さない。[S7]

Copy iconはCopy可能な詳細値の横に常時表示。各Log行や全Table cellへ機械的にアイコンを付けない。LogはToolbarのCopy、Key-valueはその値の横という単位にする。

長いPathは中央省略。全文はTooltipに加え、詳細/選択可能な表示でも参照できる。Copyするのは表示上の省略文字列ではなく元の値。外部URIやOSへの操作は許可した種別に限定し、データ値をそのまま任意コマンドとして実行しない。Copy完了は同じ場所の短い表示と控えめなLive announcementで伝える。

### Motion

Hover150ms `cubic-bezier(0.2,0,0,1)`、Menu180ms、Modal220ms `cubic-bezier(0.16,1,0.3,1)`。ModalはopacityとY方向8pxだけ。ボタン本体をHoverで移動/拡大しない。Icon移動は最大2px、Chevron回転は意味のある開閉に限定する。

Group collapseはheight＋opacity180ms。Modal移動以外の大きなPage transition、Bounce、Particle、装飾用の無限animationを追加しない。

Reduced motionでは**すべての演出を止める**。Shimmer、Spinnerの回転、Smooth scroll、Progressの補間も停止する。Spinnerだけを消してLoading意味を失わせず、状態文字と実際の進捗値を残す。[S9]

Forced colors / High contrastではシステム色とOutlineを尊重する。Box-shadowだけでFocusを表現せず、独自の強制色でOS設定を打ち消さない。[S14]

## 18. 実装に渡すための受入条件

| 検査 | 合格条件 |
|---|---|
| 1024×640 / 1440×900、Light/Dark | 標準状態でPage全体の横Scrollなし。固定操作が見える |
| 長い日本語Instance名・Path・Loader名 | 小さな文字への縮小やActionの消失なし。全文への経路あり |
| 200%拡大・実効幅320px | 本文/フォームがReflowし、Navigation/Close/Apply/Playへ到達可能 |
| 11個のInstance Tabs | マウスとキーボードの双方で全Tabへ到達でき、選択Tabが見える |
| Modal内でPlay | 準備/Download/起動中/失敗をFooterから確認できる |
| Modalを閉じた処理 | 継続中ならSidebarに同じ状態が残る。閉じるだけで取消しない |
| Save failure | 入力Draftが残る。成功・保存済みと誤表示しない |
| Esc連打/Backdrop click | 1イベント1レイヤー。未保存入力を無断破棄しない |
| Force Quit failure | 実プロセスの終了が確認できるまでNow Playingを消さない |
| Checkbox | Enabledと行選択を混同しない。小さい見た目でも32pxの操作領域 |
| Group移動 | DragなしのMenu操作でも同じ結果。Sortと見かけのDrop位置が矛盾しない |
| Galleryの白/黒/高密度画像 | Hover/Focus時のCaptionが読める。Lightboxは元の比率で表示 |
| Log更新 | 過去行閲覧やテキスト選択を妨げない。失敗文字を読める |
| 0件 / 検索0件 / 未取得 / Error | 異なる状態として適切な説明・操作を表示 |
| Offline / 認証切れ | ローカル閲覧を不必要に止めず、起動可否を偽らない |
| OS Scrollbar / Reduced motion / High contrast | ユーザー設定を妨げず、必要な情報・操作が残る |
| 表示データと操作権限 | 未実装機能、架空の進捗、未確認の安全性を正常表示しない |

検査データは、長い日本語名、200件のInstance、複数Group、多数のMod、縦長/超横長の画像、長いLog、取得失敗を含める。件数増加時は必要に応じて仮想化するが、Focus・選択・検索・件数の意味を失わせない。

## 19. トークンの数値検査

HEX色をsRGBとして相対輝度からコントラスト比を算出し、選定した**178組**を検査した。対象は指定した通常面/状態面上の文字、必要なControl境界、状態マーク、Buttonの各状態、Tooltip、Progress、Log、画像Captionの最も明るい背景条件。

次はそれぞれの対象群で比率が最小だった組み合わせ。表示値は小数点以下2桁に丸めているが、合否は丸め前の値で判定した。[S1]

| Theme | 対象 | 検査対象のうち最小となる組み合わせ | 比率 | 設計閾値 |
|---|---|---|---:|---:|
| Light | `text.secondary` | `#666666` / `#F0F0F0` | 5.04:1 | 4.5:1 |
| Light | `success.foreground` | `#166534` / `#F0F0F0` | 6.26:1 | 4.5:1 |
| Light | `danger.foreground` | `#C32932` / `#F0F0F0` | 5.00:1 | 4.5:1 |
| Light | `warning.foreground` | `#92400E` / `#F0F0F0` | 6.22:1 | 4.5:1 |
| Light | `info.foreground` | `#1D4ED8` / `#F0F0F0` | 5.88:1 | 4.5:1 |
| Light | `border.control` | `#8A8A8A` / `#F0F0F0` | 3.03:1 | 3:1 |
| Dark | `text.secondary` | `#A1A1A1` / `#202020` | 6.31:1 | 4.5:1 |
| Dark | `success.foreground` | `#4ADE80` / `#202020` | 9.35:1 | 4.5:1 |
| Dark | `danger.foreground` | `#F87171` / `#202020` | 5.89:1 | 4.5:1 |
| Dark | `warning.foreground` | `#FBBF24` / `#202020` | 9.76:1 | 4.5:1 |
| Dark | `info.foreground` | `#60A5FA` / `#202020` | 6.41:1 | 4.5:1 |
| Dark | `border.control` | `#737373` / `#202020` | 3.44:1 | 3:1 |

Captionは黒72%を白画像上に合成した最も明るい背景を8bit値の明るい側へ丸めた`#484848`でも、白文字と約9.15:1を確保する計算。これは文字の直下がこの面で覆われる場合の値であり、面からはみ出た文字には適用しない。

Disabled、装飾Separator、半透明Shadowはこの閾値検査と分けて扱う。配色計算に通っても、画面全体のWCAG適合、実際のFont描画、Focus挙動、スクリーンリーダー対応まで検証済みとはしない。

## 20. 参照資料

以下は規格・Font選定・挙動確認のための一次資料。MonaLauncherの具体的な寸法、色、保存単位、画面構成は本改訂での設計判断であり、参照元製品の仕様を転載したものではない。参照日: 2026-09-09。

[S1] W3C — Understanding SC 1.4.3 Contrast (Minimum): `https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html`

[S2] W3C — Understanding SC 1.4.11 Non-text Contrast: `https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html`

[S3] W3C WAI-ARIA APG — Dialog (Modal) Pattern: `https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/`

[S4] W3C WAI-ARIA APG — Tabs Pattern: `https://www.w3.org/WAI/ARIA/apg/patterns/tabs/`

[S5] W3C — Understanding SC 2.5.7 Dragging Movements: `https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements.html`

[S6] W3C — Understanding SC 2.5.8 Target Size (Minimum): `https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html`

[S7] W3C — Understanding SC 1.4.13 Content on Hover or Focus: `https://www.w3.org/WAI/WCAG22/Understanding/content-on-hover-or-focus.html`

[S8] MDN — scrollbar-width: `https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scrollbar-width`

[S9] MDN — prefers-reduced-motion: `https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/prefers-reduced-motion`

[S10] Vercel — Geist Font: `https://vercel.com/font`

[S11] Noto CJK official repository / Google Fonts — Noto Sans Japanese: `https://github.com/notofonts/noto-cjk` / `https://fonts.google.com/noto/specimen/Noto+Sans+JP`

[S12] W3C — WCAG 2.2 / Reflow: `https://www.w3.org/TR/WCAG22/` / `https://www.w3.org/WAI/WCAG22/Understanding/reflow`

[S13] W3C — Focus Not Obscured (Minimum): `https://www.w3.org/WAI/WCAG22/Understanding/focus-not-obscured-minimum.html`

[S14] MDN — forced-colors: `https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/forced-colors`

---

**最終原則:** 囲うことより階層、色を増やすことよりコントラスト、演出より現在の状態。ミニマルさのために、操作の入口やデータ保護を削らない。
