import type { MinecraftInstance } from "../../../domain/launcher";

export function VersionPanel({ instance }: { instance: MinecraftInstance }) {
  return (
    <>
      <h3 className="mb-4 text-section-title">Component versions</h3>
      <dl className="facts">
        <div>
          <dt>Minecraft</dt>
          <dd className="font-mono">{instance.versionId}</dd>
        </div>
        <div>
          <dt>Mod Loader</dt>
          <dd className="font-mono">
            {instance.modLoader.type === "fabric" ? `Fabric ${instance.modLoader.version}` : "None"}
          </dd>
        </div>
      </dl>
      <p className="mt-6 text-text-secondary">
        既存インスタンスのバージョン変更には未対応です。別のバージョンは新しいインスタンスとして作成してください。
      </p>
    </>
  );
}
