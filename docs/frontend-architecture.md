# フロントエンドの責務と配置

Reactの分割は、行数や見た目の小片ではなく、状態・操作・意味上の責務で決める。既存の機能部品と `src/components/` を確認してから、新しい部品を追加する。

## レイヤー

| 配置                             | 責務                                                                                  |
| -------------------------------- | ------------------------------------------------------------------------------------- |
| `src/App.tsx`                    | controller、ページ、Dialogの組み立て。検索・認証・保存の実装は持たない                |
| `src/app/`                       | Shell、Sidebar、ページ移動、アプリ全体で共有するインスタンス操作の調停                |
| `src/features/home/`             | Homeページと起動中・最近使ったインスタンスの表示                                      |
| `src/features/instances/`        | 一覧、フィルター、起動履歴、Instance Dialogのナビゲーション記憶                       |
| `src/features/instances/create/` | 作成フォーム、バージョン選択、Minecraft/Fabricカタログ取得                            |
| `src/features/instances/detail/` | 詳細Dialog、各タブ、診断、名前編集と未保存ガード、破壊操作の確認                      |
| `src/features/instances/log/`    | ログの検索・表示・Follow・Copy/Export、資格情報のマスキング                           |
| `src/features/auth/`             | アカウント表示、認証Dialog、Microsoft認証・polling・サインアウト                      |
| `src/features/mods/`             | 導入済みMod、検索・導入・削除、非同期要求と進捗の管理                                 |
| `src/features/settings/`         | 設定ページ、外観の保存・適用                                                          |
| `src/components/`                | 機能を問わず再利用するButton、Dialog、Tabs、Empty、ErrorMessage、CopyButton、Progress |
| `src/domain/launcher.ts`         | IPCで共有するデータ型。Reactの状態や画面には依存しない                                |
| `src/lib/tauri.ts`               | 実行環境の判定                                                                        |

`useLauncher` は既存の公開controllerを維持しつつ、`useAuthentication`、`useModManagement`、`useVersionCatalog` を合成する。IPC呼び出しとイベント購読を表示コンポーネントへ戻さない。末端の部品は `Pick<Launcher, ...>` または直接のデータpropsで、必要な状態と操作を明示する。例えば `LogPanel` はログ行だけを受け取り、認証・削除・起動APIには依存しない。

## 状態の寿命

- 認証・進行中のインスタンス/Mod操作はアプリの寿命で保持する。Dialogを閉じても処理は失われない。
- 一覧の検索・フィルター・Sortは `useInstanceFilters` に保持し、ページの再表示で失わない。
- 起動履歴は `usePlayHistory`、Themeは `appearance.ts` と `AppearanceSettings` が扱う。初回描画も同じTheme読込関数を使う。
- 開いているInstance、前回TabとScroll位置は `useInstanceNavigation` が管理する。
- 名前の未保存ガードは `useInstanceEditor` がDialog全体で保持する。タブの表示部品がunmountされても、保存失敗でDraftを破棄しない。
- 削除確認の名前入力は `InstanceActionConfirmation` の寿命だけ保持する。
- Releaseフィルターは `VersionSelector`、ログ検索・Wrap・Followは `LogPanel` に置く。

## 追加・変更時の基準

1. ページは機能部品の合成を中心にする。
2. 独立した操作、状態、意味、再利用単位がある場合にコンポーネントを抽出する。行数を減らすだけのwrapperを作らない。
3. 特定画面や機能の部品はそのfeature内に置く。`components` に機能固有のDialogを集めない。
4. 複雑なデータ処理や非同期処理はhook・utility・domainへ置く。props経由で必要な能力を明示する。
5. 実装後に、複数の責務が蓄積した部品を見直す。共通化のためだけに用途別の保存・削除ルールを一つの汎用Dialogへ押し込まない。

検証は `pnpm check` と `pnpm test:ui`。構造変更でも、Focus、未保存編集、非同期処理の継続、テーマ保存を維持する。デザインの値と操作条件は [design/README.md](../design/README.md) を参照する。

## 共通UIとBase UI

`src/components/` のButton、Dialog、Tabs、Progressは `@base-ui/react` の各primitiveを使用する。CopyButtonは共通Buttonを合成する。EmptyとErrorMessageは表示専用の意味を持つHTMLを維持する。見た目はデザイントークンと `src/App.css` で定義し、Base UIにはフォーカス管理、キーボード操作、ARIAと状態の関連付けを任せる。

Dialogは呼び出し元の条件付き表示を維持するcontrolled component。閉じる要求は `onClose` を通し、未保存ガードと処理中の閉じ方はfeature側で決める。確認・Mod管理などの子Dialogは親DialogのReactツリー内に置き、Base UIがレイヤーの親子関係を認識できるようにする。初期Focusは `data-initial-focus`、未指定ならタイトル。未保存ガードを閉じた際は編集中のPanelへFocusを戻し、拒否したTab移動がFocus復帰で再発しないようにする。Portalの配色・文字・寸法は共通CSSで指定する。

TabsはTablistとPanelをまとめ、表示内容はchildren、Panelのref・Scroll記憶・見た目は `panelProps` で受け取る。選択状態と未保存ガードは引き続きfeatureが所有する。Progressは未確定値を `null` としてBase UIに渡し、数値がない場合は%や塗りつぶし量を表示しない。

Permissionsの保存済み値はIPCの `MinecraftInstance.permissions` に保持し、更新は `useLauncher` が調停する。`usePermissionSupport` はバックエンドの対応状況を取得する。`PermissionsPanel` は操作中・再試行の表示を所有し、確定値を直接変更しない。保存操作は `InstanceDialog` から `useInstanceSaveToast` を通し、保存中の通知を結果へ更新する。Toast viewportは画面右下へfixed配置する。DOM上はモーダルのFocus境界内に保持し、通常・成功・失敗・警告の通知を共通部品で描画する。タブやDialogを閉じても開始済みの保存結果はcontrollerのインスタンス一覧へ反映される。共通SwitchはBase UIを使い、値の保存・権限の意味はfeature側に残す。
