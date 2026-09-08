import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  InstalledMod,
  MinecraftInstance,
  ModInstallProgress,
  ModInstallResult,
  ModRemovalResult,
  ModSearchResponse,
} from "../../domain/launcher";
import { hasTauriRuntime } from "../../lib/tauri";
export function useModManagement(selected: MinecraftInstance | null, isRunning: boolean) {
  const [showMods, setShowMods] = useState(false);

  const [modQuery, setModQuery] = useState("");

  const [modSearch, setModSearch] = useState<ModSearchResponse | null>(null);

  const [modSearchLoading, setModSearchLoading] = useState(false);

  const [installedMods, setInstalledMods] = useState<InstalledMod[]>([]);

  const [installedModsLoading, setInstalledModsLoading] = useState(false);

  const [modInstallingProjectId, setModInstallingProjectId] = useState<string | null>(null);

  const [modRemovingProjectId, setModRemovingProjectId] = useState<string | null>(null);

  const [modRemovalTarget, setModRemovalTarget] = useState<InstalledMod | null>(null);

  const [modInstallProgress, setModInstallProgress] = useState<ModInstallProgress | null>(null);

  const [modError, setModError] = useState<string | null>(null);

  const [modSuccess, setModSuccess] = useState<string | null>(null);

  const modSearchRequestRef = useRef(0);

  const installedModsRequestRef = useRef(0);

  const installedProjectIds = useMemo(
    () => new Set(installedMods.map((item) => item.projectId)),
    [installedMods],
  );

  const modOperationActive = modInstallingProjectId !== null || modRemovingProjectId !== null;

  const currentModInstallProgress =
    modInstallProgress?.instanceId === selected?.id ? modInstallProgress : null;

  const modInstallPercent = currentModInstallProgress
    ? currentModInstallProgress.total === 0
      ? 0
      : Math.min(
          100,
          Math.max(
            0,
            Math.round(
              (currentModInstallProgress.completed / currentModInstallProgress.total) * 100,
            ),
          ),
        )
    : 0;

  const refreshInstalledMods = useCallback(async (instanceId: string) => {
    const requestId = installedModsRequestRef.current + 1;
    installedModsRequestRef.current = requestId;
    setInstalledModsLoading(true);
    try {
      const mods = await invoke<InstalledMod[]>("list_instance_mods", { instanceId });
      if (installedModsRequestRef.current === requestId) setInstalledMods(mods);
    } catch (cause) {
      if (installedModsRequestRef.current === requestId) throw cause;
    } finally {
      if (installedModsRequestRef.current === requestId) setInstalledModsLoading(false);
    }
  }, []);

  const runModSearch = async (instanceId: string, query: string, offset: number) => {
    const requestId = modSearchRequestRef.current + 1;
    modSearchRequestRef.current = requestId;
    setModSearchLoading(true);
    setModError(null);
    try {
      const result = await invoke<ModSearchResponse>("search_modrinth_mods", {
        instanceId,
        query,
        offset,
      });
      if (modSearchRequestRef.current === requestId) setModSearch(result);
    } catch (cause) {
      if (modSearchRequestRef.current === requestId) {
        setModSearch(null);
        setModError(String(cause));
      }
    } finally {
      if (modSearchRequestRef.current === requestId) setModSearchLoading(false);
    }
  };

  const openMods = () => {
    if (!selected || selected.modLoader.type !== "fabric" || isRunning) return;
    setModQuery("");
    setModSearch(null);
    setInstalledMods([]);
    setModInstallProgress(null);
    setModRemovingProjectId(null);
    setModRemovalTarget(null);
    setModError(null);
    setModSuccess(null);
    setShowMods(true);
    void refreshInstalledMods(selected.id).catch((cause) => setModError(String(cause)));
    void runModSearch(selected.id, "", 0);
  };

  const installModrinthMod = async (projectId: string) => {
    if (!selected || modOperationActive) return;
    setModRemovalTarget(null);
    setModInstallingProjectId(projectId);
    setModInstallProgress({
      instanceId: selected.id,
      completed: 0,
      total: 0,
      message: "必須依存関係を確認しています…",
    });
    setModError(null);
    setModSuccess(null);
    try {
      const result = await invoke<ModInstallResult>("install_modrinth_mod", {
        instanceId: selected.id,
        projectId,
      });
      const direct = result.installed.find((item) => item.projectId === projectId);
      const dependencies = result.installed.filter((item) => !item.direct).length;
      setModSuccess(
        direct
          ? `${direct.title} ${direct.versionNumber}を導入しました${dependencies > 0 ? `（必須依存${dependencies}件を含む）` : ""}。`
          : `Modを導入しました（${result.installed.length}ファイル）。`,
      );
      try {
        await refreshInstalledMods(selected.id);
      } catch (cause) {
        setModError(`導入は完了しましたが、一覧を更新できませんでした: ${String(cause)}`);
      }
    } catch (cause) {
      setModError(String(cause));
    } finally {
      setModInstallingProjectId(null);
      setModInstallProgress(null);
    }
  };

  const removeModrinthMod = async (target: InstalledMod) => {
    if (!selected || !target.direct || modOperationActive) return;
    setModRemovingProjectId(target.projectId);
    setModError(null);
    setModSuccess(null);
    try {
      const result = await invoke<ModRemovalResult>("remove_modrinth_mod", {
        instanceId: selected.id,
        projectId: target.projectId,
      });
      const orphanedDependencies = result.removed.filter(
        (item) => item.projectId !== result.requested.projectId,
      ).length;
      const cleanupNote = result.cleanupPending
        ? " ゲームのmodsフォルダからは外しましたが、一時退避ファイルの後片付けが残っています。"
        : "";
      setModSuccess(
        result.retainedAsDependency
          ? `${result.requested.title}の直接追加を解除しました。別のModに必要なため、JARは必須依存として残しています。${cleanupNote}`
          : `${result.requested.title}を削除しました${
              orphanedDependencies > 0
                ? `（不要になった必須依存${orphanedDependencies}件も削除）`
                : ""
            }。${cleanupNote}`,
      );
      setModRemovalTarget(null);
      try {
        await refreshInstalledMods(selected.id);
      } catch (cause) {
        setModError(`削除は完了しましたが、一覧を更新できませんでした: ${String(cause)}`);
      }
    } catch (cause) {
      setModError(String(cause));
    } finally {
      setModRemovingProjectId(null);
    }
  };
  useEffect(() => {
    setInstalledMods([]);
    setModSearch(null);
    setModRemovalTarget(null);
    setModInstallProgress(null);
    setShowMods(false);
    setModSearchLoading(false);
    modSearchRequestRef.current += 1;
    setModError(null);
    setModSuccess(null);
    installedModsRequestRef.current += 1;
  }, [selected?.id]);
  useEffect(() => {
    if (!hasTauriRuntime()) return;
    const unlisten = listen<ModInstallProgress>("modrinth-install-progress", (event) =>
      setModInstallProgress(event.payload),
    );
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);
  return {
    showMods,
    setShowMods,
    modQuery,
    setModQuery,
    modSearch,
    modSearchLoading,
    installedMods,
    installedModsLoading,
    modInstallingProjectId,
    modRemovingProjectId,
    modRemovalTarget,
    setModRemovalTarget,
    modInstallProgress,
    modError,
    setModError,
    modSuccess,
    installedProjectIds,
    modOperationActive,
    currentModInstallProgress,
    modInstallPercent,
    refreshInstalledMods,
    runModSearch,
    openMods,
    installModrinthMod,
    removeModrinthMod,
  };
}
