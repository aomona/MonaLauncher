import type { MinecraftInstance } from "../../domain/launcher";
export function instanceVersionLabel(instance: MinecraftInstance) {
  return instance.modLoader.type === "fabric"
    ? `Minecraft ${instance.versionId} · Fabric ${instance.modLoader.version}`
    : `Minecraft ${instance.versionId}`;
}
