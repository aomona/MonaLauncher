import type { InstancePermissions, PermissionSupport } from "../../../domain/launcher";

type Permission = {
  key: keyof InstancePermissions;
  label: string;
  description: string;
  file?: boolean;
  support?: "audioOutput" | "microphone" | "clipboard";
};
export const permissionGroups: { label: string; items: Permission[] }[] = [
  {
    label: "ファイル",
    items: [
      {
        key: "gameWrite",
        label: "ゲームデータへの書き込み",
        description:
          "ゲーム全体の書き込みを許可します。OFFにすると下の項目もすべて読み取り専用になります。ルートのoptions.txtなどはこの設定に従います。",
      },
      {
        key: "worldsWrite",
        label: "ワールドの保存",
        description:
          "saves内の作成・更新・削除を許可します。OFFではワールドが開けない場合があります。",
        file: true,
      },
      {
        key: "screenshotsWrite",
        label: "スクリーンショットの保存",
        description: "screenshots内への画像の保存・削除を許可します。",
        file: true,
      },
      {
        key: "resourcePacksWrite",
        label: "リソースパックの変更",
        description: "resourcepacks内の作成・更新・削除を許可します。読み込みはOFFでも可能です。",
        file: true,
      },
      {
        key: "shaderPacksWrite",
        label: "シェーダーパックの変更",
        description: "shaderpacks内の作成・更新・削除を許可します。読み込みはOFFでも可能です。",
        file: true,
      },
      {
        key: "modsWrite",
        label: "Modファイルの変更",
        description:
          "ゲームやModによるmods内の変更を許可します。ランチャーからのMod管理には影響しません。",
        file: true,
      },
      {
        key: "configWrite",
        label: "Mod設定の保存",
        description:
          "config内の設定ファイルの作成・更新・削除を許可します。別の場所に保存するModは対象外です。",
        file: true,
      },
      {
        key: "logsWrite",
        label: "ログファイルの保存",
        description: "logs内への書き込みを許可します。OFFでは起動に失敗する場合があります。",
        file: true,
      },
    ],
  },
  {
    label: "通信とデスクトップ",
    items: [
      {
        key: "network",
        label: "ネットワーク通信",
        description:
          "インターネット・LANへの送受信を許可します。すべてのModにも適用され、読み取れるゲームデータを外部へ送信できるようになります。Windowsのlocalhost制限は別に適用されます。",
      },
      {
        key: "audioOutput",
        label: "通常音声",
        description:
          "ゲームの効果音・音楽に必要な音声サービスへの接続を許可します。ナレーターは別の設定です。Linuxではこの接続に録音機能へのアクセスも含まれます。",
        support: "audioOutput",
      },
      {
        key: "microphone",
        label: "マイク",
        description:
          "マイク使用をサンドボックスで許可します。通常音声もONにする必要があり、macOS側のプライバシー許可は別途必要です。",
        support: "microphone",
      },
      {
        key: "clipboard",
        label: "クリップボード",
        description:
          "ホストのクリップボードサービスへの接続を許可します。コピーした内容の読み取りと書き換えが可能になります。",
        support: "clipboard",
      },
      {
        key: "narrator",
        label: "ナレーター",
        description:
          "ゲームとModからのテキスト読み上げを許可します。通常の効果音や音楽には影響しません。",
      },
    ],
  },
];

export function unavailableReason(item: Permission, support: PermissionSupport | null) {
  if (!item.support || support?.[item.support]) return null;
  if (!support) return "対応状況を確認しています…";
  if (item.key === "audioOutput") return "Windowsでは通常音声の接続を個別に遮断できません。";
  if (item.key === "microphone")
    return support?.platform === "linux"
      ? "Linuxでは通常音声と同じPulseAudio接続に含まれ、録音だけの制御は未対応です。"
      : "このOSでのマイク権限の個別変更は未対応です。Windowsではマイク用capabilityを付与しません。";
  return "このOSでは画面接続からクリップボードだけを分離する制御は未対応です。利用可否はOS・画面サービス側の制約に従います。";
}
