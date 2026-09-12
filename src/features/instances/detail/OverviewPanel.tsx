import type { Launcher } from "../../../app/useLauncher";
import { CopyButton } from "../../../components/CopyButton";

import { InstanceDiagnostics } from "./InstanceDiagnostics";
export function OverviewPanel({
  launcher: l,
  onRepair,
}: {
  launcher: Launcher;
  onRepair: () => void;
}) {
  const instance = l.selected!;
  return (
    <>
      <h3 className="mb-4 text-section-title">基本情報</h3>
      <dl className="facts">
        <div>
          <dt>Minecraft</dt>
          <dd className="font-mono">{instance.versionId}</dd>
        </div>
        <div>
          <dt>Mod Loader</dt>
          <dd className="font-mono">
            {instance.modLoader.type === "fabric"
              ? `Fabric ${instance.modLoader.version}`
              : "Vanilla"}
          </dd>
        </div>
        <div>
          <dt>Game directory</dt>
          <dd>
            <span className="font-mono">{instance.gameDirectory}</span>
            <CopyButton value={instance.gameDirectory} label="ゲームディレクトリ" />
          </dd>
        </div>
        <div>
          <dt>実行方式</dt>
          <dd>{instance.sandboxed ? "OSサンドボックス · ネットワーク権限なし" : "旧形式"}</dd>
        </div>
        <div>
          <dt>ゲームモード</dt>
          <dd>{instance.demo ? "デモ" : "通常"}</dd>
        </div>
      </dl>
      <InstanceDiagnostics launcher={l} onRepair={onRepair} />
    </>
  );
}
