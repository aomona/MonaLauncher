import { invoke } from "@tauri-apps/api/core";
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import type { FabricLoaderVersion, MinecraftVersionCatalog } from "../../../domain/launcher";
import { hasTauriRuntime } from "../../../lib/tauri";
export function useVersionCatalog(
  showCreator: boolean,
  setCreatorError: Dispatch<SetStateAction<string | null>>,
) {
  const [versionCatalog, setVersionCatalog] = useState<MinecraftVersionCatalog | null>(null);

  const [selectedVersionId, setSelectedVersionId] = useState("");

  const [versionQuery, setVersionQuery] = useState("");

  const [versionsLoading, setVersionsLoading] = useState(false);

  const [modLoaderType, setModLoaderType] = useState<"vanilla" | "fabric">("vanilla");

  const [fabricLoaders, setFabricLoaders] = useState<FabricLoaderVersion[]>([]);

  const [selectedFabricLoader, setSelectedFabricLoader] = useState("");

  const [fabricLoadersLoading, setFabricLoadersLoading] = useState(false);

  const [fabricLoadersError, setFabricLoadersError] = useState<string | null>(null);

  const [fabricReloadKey, setFabricReloadKey] = useState(0);

  const [versionsError, setVersionsError] = useState<string | null>(null);

  const versionGroups = useMemo(() => {
    const groups = [
      { type: "release", label: "正式リリース" },
      { type: "snapshot", label: "スナップショット" },
      { type: "old_beta", label: "旧Beta" },
      { type: "old_alpha", label: "旧Alpha" },
    ];
    const query = versionQuery.trim().toLowerCase();

    return groups.map((group) => ({
      ...group,
      versions: (versionCatalog?.versions ?? []).filter(
        (version) =>
          version.versionType === group.type &&
          (!query || version.id.toLowerCase().includes(query) || version.id === selectedVersionId),
      ),
    }));
  }, [selectedVersionId, versionCatalog, versionQuery]);

  const refreshVersions = useCallback(async () => {
    setCreatorError(null);
    setVersionsError(null);
    setVersionsLoading(true);
    try {
      const catalog = await invoke<MinecraftVersionCatalog>("list_minecraft_versions");
      setVersionCatalog(catalog);
      setSelectedVersionId((current) =>
        catalog.versions.some((version) => version.id === current)
          ? current
          : catalog.latest.release,
      );
    } catch (cause) {
      setVersionsError(String(cause));
      throw cause;
    } finally {
      setVersionsLoading(false);
    }
  }, [setCreatorError]);

  useEffect(() => {
    if (!showCreator || modLoaderType !== "fabric" || !selectedVersionId || !hasTauriRuntime()) {
      setFabricLoaders([]);
      setSelectedFabricLoader("");
      setFabricLoadersLoading(false);
      setFabricLoadersError(null);
      return;
    }

    let cancelled = false;
    setFabricLoadersLoading(true);
    setFabricLoadersError(null);
    void invoke<FabricLoaderVersion[]>("list_fabric_loader_versions", {
      minecraftVersion: selectedVersionId,
    })
      .then((loaders) => {
        if (cancelled) return;
        setFabricLoaders(loaders);
        setSelectedFabricLoader((current) =>
          loaders.some((loader) => loader.version === current)
            ? current
            : (loaders.find((loader) => loader.stable)?.version ?? loaders[0]?.version ?? ""),
        );
      })
      .catch((cause) => {
        if (cancelled) return;
        setFabricLoaders([]);
        setSelectedFabricLoader("");
        setFabricLoadersError(String(cause));
      })
      .finally(() => {
        if (!cancelled) setFabricLoadersLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [fabricReloadKey, modLoaderType, selectedVersionId, showCreator]);
  return {
    versionCatalog,
    selectedVersionId,
    setSelectedVersionId,
    versionQuery,
    setVersionQuery,
    versionsLoading,
    modLoaderType,
    setModLoaderType,
    fabricLoaders,
    selectedFabricLoader,
    setSelectedFabricLoader,
    fabricLoadersLoading,
    fabricLoadersError,
    setFabricReloadKey,
    versionsError,
    versionGroups,
    refreshVersions,
  };
}
