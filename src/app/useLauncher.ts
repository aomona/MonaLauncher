import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useRef, useState } from "react";
import type {
  InstallProgress,
  InstanceDiagnosis,
  LogLine,
  MinecraftInstance,
  MinecraftLaunchProgress,
  MinecraftLogEvent,
  MinecraftStatusEvent,
} from "../domain/launcher";
import { useAuthentication } from "../features/auth/useAuthentication";
import { useVersionCatalog } from "../features/instances/create/useVersionCatalog";
import { redactLog } from "../features/instances/log/log-utils";
import { useModManagement } from "../features/mods/useModManagement";
import { hasTauriRuntime } from "../lib/tauri";
export function useLauncher() {
  const [instances, setInstances] = useState<MinecraftInstance[]>([]);

  const [instancesLoading, setInstancesLoading] = useState(true);

  const [selectedId, setSelectedId] = useState("");

  const [instanceId, setInstanceId] = useState("minecraft");

  const [instanceName, setInstanceName] = useState("Minecraft");

  const [gameMode, setGameMode] = useState<"offline" | "demo">("offline");

  const [progress, setProgress] = useState<InstallProgress | null>(null);

  const [launchProgress, setLaunchProgress] = useState<MinecraftLaunchProgress | null>(null);

  const [logs, setLogs] = useState<LogLine[]>([]);

  const [runningIds, setRunningIds] = useState<Set<string>>(new Set());

  const [busy, setBusy] = useState<
    "install" | "launch" | "stop" | "rename" | "delete" | "diagnose" | "repair" | null
  >(null);

  const [error, setError] = useState<string | null>(null);

  const [creatorError, setCreatorError] = useState<string | null>(null);

  const [showCreator, setShowCreator] = useState(false);

  const [settingsName, setSettingsName] = useState("");

  const [diagnosis, setDiagnosis] = useState<InstanceDiagnosis | null>(null);

  const stoppingIdRef = useRef<string | null>(null);

  const selected = useMemo(
    () => instances.find((instance) => instance.id === selectedId) ?? null,
    [instances, selectedId],
  );

  const isRunning = selected ? runningIds.has(selected.id) : false;

  const auth = useAuthentication();
  const mods = useModManagement(selected, isRunning);
  const catalog = useVersionCatalog(showCreator, setCreatorError);
  const { refreshVersions, selectedVersionId, modLoaderType, selectedFabricLoader } = catalog;
  const visibleLogs = selected ? logs.filter((line) => line.instanceId === selected.id) : [];

  const progressPercent = progress
    ? progress.total === 0
      ? 0
      : Math.min(100, Math.max(0, Math.round((progress.completed / progress.total) * 100)))
    : 0;

  const openCreator = () => {
    setInstanceId(`instance-${crypto.randomUUID()}`);
    setCreatorError(null);
    setProgress(null);
    setShowCreator(true);
  };

  const refreshInstances = async () => {
    setError(null);
    setInstancesLoading(true);
    try {
      const found = await invoke<MinecraftInstance[]>("list_minecraft_instances");
      setInstances(found);
      setSelectedId((current) => {
        if (found.some((instance) => instance.id === current)) return current;
        return found[0]?.id ?? "";
      });
    } finally {
      setInstancesLoading(false);
    }
  };

  useEffect(() => {
    if (!hasTauriRuntime()) {
      setInstancesLoading(false);
      return;
    }

    void refreshInstances().catch((cause) => setError(String(cause)));
    void refreshVersions().catch((cause) => setError(String(cause)));

    const unlistenProgress = listen<InstallProgress>("minecraft-install-progress", (event) => {
      setProgress(event.payload);
    });
    const unlistenLogs = listen<MinecraftLogEvent>("minecraft-log", (event) => {
      setLogs((current) => [
        ...current.slice(-999),
        { ...event.payload, line: redactLog(event.payload.line), id: Date.now() + Math.random() },
      ]);
    });
    const unlistenLaunchProgress = listen<MinecraftLaunchProgress>(
      "minecraft-launch-progress",
      (event) => setLaunchProgress(event.payload),
    );

    const unlistenStatus = listen<MinecraftStatusEvent>("minecraft-status", (event) => {
      setRunningIds((current) => {
        const next = new Set(current);
        if (event.payload.status === "running") next.add(event.payload.instanceId);
        else next.delete(event.payload.instanceId);
        return next;
      });

      if (event.payload.status === "stopped") {
        if (stoppingIdRef.current === event.payload.instanceId) {
          stoppingIdRef.current = null;
          setBusy((current) => (current === "stop" ? null : current));
        }
        const suffix =
          event.payload.exitCode === null ? "" : ` (終了コード ${event.payload.exitCode})`;
        setLogs((current) => [
          ...current.slice(-999),
          {
            id: Date.now() + Math.random(),
            instanceId: event.payload.instanceId,
            stream: "launcher",
            line: `Minecraftが終了しました${suffix}`,
          },
        ]);
      }
    });

    return () => {
      void unlistenProgress.then((unlisten) => unlisten());
      void unlistenLogs.then((unlisten) => unlisten());
      void unlistenLaunchProgress.then((unlisten) => unlisten());

      void unlistenStatus.then((unlisten) => unlisten());
    };
  }, [refreshVersions]);

  useEffect(() => {
    setDiagnosis(null);
    setError(null);
  }, [selectedId]);

  const install = async () => {
    setCreatorError(null);
    setProgress({ stage: "metadata", completed: 0, total: 0, message: "準備中" });
    setBusy("install");

    try {
      const installed = await invoke<MinecraftInstance>("install_sandbox_instance", {
        instanceId,
        name: instanceName,
        versionId: selectedVersionId,
        demo: gameMode === "demo",
        modLoader:
          modLoaderType === "fabric"
            ? { type: "fabric", version: selectedFabricLoader }
            : { type: "vanilla" },
      });
      await refreshInstances();
      setSelectedId(installed.id);
      setShowCreator(false);
      return installed;
    } catch (cause) {
      setCreatorError(String(cause));
    } finally {
      setProgress(null);
      setBusy(null);
    }
  };

  const launch = async (target = selected) => {
    if (!target || busy || runningIds.has(target.id)) return;
    setSelectedId(target.id);
    setError(null);
    setBusy("launch");
    setLaunchProgress({
      instanceId: target.id,
      stage: "queued",
      message: "起動処理を開始しています…",
    });
    setLogs((current) => current.filter((line) => line.instanceId !== target.id));

    try {
      const pid = await invoke<number>("launch_minecraft_instance", {
        instanceId: target.id,
      });
      setLogs((current) => [
        ...current,
        {
          id: Date.now(),
          instanceId: target.id,
          stream: "launcher",
          line: `Minecraftを起動しました (PID ${pid})`,
        },
      ]);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLaunchProgress(null);
      setBusy(null);
    }
  };

  const stop = async () => {
    if (!selected || busy || !runningIds.has(selected.id)) return;
    setError(null);
    stoppingIdRef.current = selected.id;
    setBusy("stop");
    try {
      await invoke("stop_minecraft_instance", { instanceId: selected.id });
      // Keep Stopping until the process-status event confirms termination.
    } catch (cause) {
      stoppingIdRef.current = null;
      setError(String(cause));
      setBusy(null);
    }
  };

  const diagnoseSelected = async () => {
    if (!selected) return;
    setError(null);
    setBusy("diagnose");
    try {
      const result = await invoke<InstanceDiagnosis>("diagnose_minecraft_instance", {
        instanceId: selected.id,
      });
      setDiagnosis(result);
    } catch (cause) {
      setDiagnosis(null);
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const repairSelected = async () => {
    if (!selected) return;
    setError(null);
    setProgress({ stage: "metadata", completed: 0, total: 0, message: "修復準備中" });
    setBusy("repair");
    try {
      const result = await invoke<InstanceDiagnosis>("repair_minecraft_instance", {
        instanceId: selected.id,
      });
      setDiagnosis(result);
      await refreshInstances();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setProgress(null);
      setBusy(null);
    }
  };

  const renameSelected = async () => {
    if (!selected) return;
    setError(null);
    setBusy("rename");

    try {
      const renamed = await invoke<MinecraftInstance>("rename_minecraft_instance", {
        instanceId: selected.id,
        name: settingsName,
      });
      setSettingsName(renamed.name);
      setInstances((current) =>
        current
          .map((instance) => (instance.id === renamed.id ? renamed : instance))
          .sort((left, right) => left.name.localeCompare(right.name, "ja")),
      );
      return true;
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const deleteSelected = async () => {
    if (!selected) return;
    const deletedId = selected.id;
    setError(null);
    setBusy("delete");

    try {
      await invoke("delete_minecraft_instance", { instanceId: deletedId });
      const remaining = instances.filter((instance) => instance.id !== deletedId);
      setInstances(remaining);
      setSelectedId((selectedId) =>
        selectedId === deletedId ? (remaining[0]?.id ?? "") : selectedId,
      );
      setLogs((current) => current.filter((line) => line.instanceId !== deletedId));
      setRunningIds((current) => {
        const next = new Set(current);
        next.delete(deletedId);
        return next;
      });
      return true;
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };
  return {
    ...auth,
    ...mods,
    ...catalog,
    refreshInstances,
    instances,
    instancesLoading,
    setSelectedId,
    instanceName,
    setInstanceName,
    gameMode,
    setGameMode,
    progress,
    launchProgress,
    runningIds,
    busy,
    error,
    setError,
    creatorError,
    showCreator,
    setShowCreator,
    settingsName,
    setSettingsName,
    diagnosis,
    selected,
    isRunning,
    visibleLogs,
    progressPercent,
    openCreator,
    install,
    launch,
    stop,
    diagnoseSelected,
    repairSelected,
    renameSelected,
    deleteSelected,
  };
}
export type Launcher = ReturnType<typeof useLauncher>;
