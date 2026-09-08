import { useState } from "react";
import type { Launcher } from "../../../app/useLauncher";
import { Button, ErrorMessage } from "../../../components/ui";
type VersionSelectorModel = Pick<
  Launcher,
  | "busy"
  | "fabricLoaders"
  | "fabricLoadersError"
  | "fabricLoadersLoading"
  | "modLoaderType"
  | "refreshVersions"
  | "selectedFabricLoader"
  | "selectedVersionId"
  | "setFabricReloadKey"
  | "setModLoaderType"
  | "setSelectedFabricLoader"
  | "setSelectedVersionId"
  | "setVersionQuery"
  | "versionCatalog"
  | "versionGroups"
  | "versionQuery"
  | "versionsError"
  | "versionsLoading"
>;
export function VersionSelector({ launcher: l }: { launcher: VersionSelectorModel }) {
  const selectedType = l.versionCatalog?.versions.find(
    (version) => version.id === l.selectedVersionId,
  )?.versionType;
  const [releaseOnly, setReleaseOnly] = useState(
    () => selectedType === undefined || selectedType === "release",
  );
  return (
    <>
      <label className="field">
        Minecraft versionを検索
        <input
          value={l.versionQuery}
          onChange={(event) => l.setVersionQuery(event.target.value)}
          placeholder="例: 1.21"
        />
      </label>
      <label className="flex min-h-8 items-center gap-2">
        <input
          type="checkbox"
          checked={releaseOnly}
          onChange={(event) => {
            setReleaseOnly(event.target.checked);
            if (
              event.target.checked &&
              l.versionCatalog?.versions.find((version) => version.id === l.selectedVersionId)
                ?.versionType !== "release"
            )
              l.setSelectedVersionId(l.versionCatalog?.latest.release ?? "");
          }}
        />
        Releaseのみ
      </label>
      <label className="field">
        Minecraft version
        <select
          required
          value={l.selectedVersionId}
          disabled={l.versionsLoading || Boolean(l.busy)}
          onChange={(event) => l.setSelectedVersionId(event.target.value)}
        >
          {!l.versionCatalog && <option value="">バージョンを取得できません</option>}
          {l.versionGroups
            .filter((group) => !releaseOnly || group.type === "release")
            .map((group) => (
              <optgroup key={group.type} label={group.label}>
                {group.versions.map((version) => (
                  <option key={version.id} value={version.id}>
                    {version.id}
                    {version.id === l.versionCatalog?.latest.release ? " — Latest release" : ""}
                  </option>
                ))}
              </optgroup>
            ))}
        </select>
      </label>
      {l.versionsLoading && <output>バージョンを読み込んでいます…</output>}
      <ErrorMessage>{l.versionsError}</ErrorMessage>
      {l.versionsError && (
        <Button onClick={() => void l.refreshVersions().catch(() => {})}>再試行</Button>
      )}
      <label className="field">
        Mod Loader
        <select
          value={l.modLoaderType}
          disabled={Boolean(l.busy)}
          onChange={(event) => l.setModLoaderType(event.target.value as "vanilla" | "fabric")}
        >
          <option value="vanilla">None</option>
          <option value="fabric">Fabric</option>
        </select>
      </label>
      {l.modLoaderType === "fabric" && (
        <>
          <label className="field">
            Fabric Loader
            <select
              value={l.selectedFabricLoader}
              disabled={l.fabricLoadersLoading || Boolean(l.busy)}
              onChange={(event) => l.setSelectedFabricLoader(event.target.value)}
            >
              {!l.fabricLoaders.length && (
                <option value="">
                  {l.fabricLoadersLoading ? "取得中…" : "対応するLoaderがありません"}
                </option>
              )}
              {l.fabricLoaders.map((loader) => (
                <option key={loader.version}>{loader.version}</option>
              ))}
            </select>
          </label>
          <ErrorMessage>{l.fabricLoadersError}</ErrorMessage>
          {l.fabricLoadersError && (
            <Button onClick={() => l.setFabricReloadKey((key) => key + 1)}>Loaderを再取得</Button>
          )}
        </>
      )}
    </>
  );
}
