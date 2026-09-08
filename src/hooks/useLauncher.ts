import { useEffect, useMemo, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { redactLog } from "../log-utils";
import { openUrl } from "@tauri-apps/plugin-opener";

export type MinecraftInstance = {
  id: string;
  name: string;
  versionId: string;
  javaPath: string;
  gameDirectory: string;
  demo: boolean;
  sandboxed: boolean;
  modLoader:
    | {
        type: "vanilla";
      }
    | {
        type: "fabric";
        version: string;
      };
};

type MinecraftVersion = {
  id: string;
  versionType: "release" | "snapshot" | "old_beta" | "old_alpha" | string;
  releaseTime: string;
};

type MinecraftVersionCatalog = {
  latest: {
    release: string;
    snapshot: string;
  };
  versions: MinecraftVersion[];
};

type FabricLoaderVersion = {
  version: string;
  stable: boolean;
};

type InstallProgress = {
  stage: string;
  completed: number;
  total: number;
  message: string;
};

type MinecraftLogEvent = {
  instanceId: string;
  stream: string;
  line: string;
};

type MinecraftStatusEvent = {
  instanceId: string;
  status: "running" | "stopped";
  exitCode: number | null;
};

type MinecraftLaunchProgress = {
  instanceId: string;
  stage: string;
  message: string;
};

type MicrosoftAuthStatus = {
  configured: boolean;
  authorized: boolean;
};

type MicrosoftSignInChallenge = {
  sessionId: string;
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
};

type MicrosoftSignInPoll = {
  status: "pending" | "authorized";
  retryAfter: number | null;
};

type MinecraftAccountProfile = {
  name: string;
  uuid: string;
};

type ModSearchHit = {
  projectId: string;
  slug: string | null;
  title: string;
  description: string;
  author: string;
  downloads: number;
  follows: number;
  dateModified: string;
};

type ModSearchResponse = {
  hits: ModSearchHit[];
  offset: number;
  limit: number;
  totalHits: number;
};

type InstalledMod = {
  projectId: string;
  versionId: string;
  title: string;
  versionNumber: string;
  fileName: string;
  sha512: string;
  size: number;
  direct: boolean;
  requiredDependencies?: string[] | null;
};

type ModInstallResult = {
  installed: InstalledMod[];
};

type ModRemovalResult = {
  requested: InstalledMod;
  removed: InstalledMod[];
  retainedAsDependency: boolean;
  cleanupPending: boolean;
};

type ModInstallProgress = {
  instanceId: string;
  completed: number;
  total: number;
  message: string;
};

type DiagnosticCheck = {
  id: string;
  label: string;
  status: "ok" | "warning" | "error";
  detail: string;
  repairable: boolean;
};

type InstanceDiagnosis = {
  instanceId: string;
  status: "healthy" | "repairable" | "attention";
  checkedFiles: number;
  issueCount: number;
  repairableCount: number;
  checks: DiagnosticCheck[];
};

export type LogLine = MinecraftLogEvent & { id: number };

export function hasTauriRuntime() {
  const internals = (
    window as typeof window & {
      __TAURI_INTERNALS__?: { invoke?: unknown; transformCallback?: unknown };
    }
  ).__TAURI_INTERNALS__;

  return (
    isTauri() &&
    typeof internals?.invoke === "function" &&
    typeof internals.transformCallback === "function"
  );
}

export function instanceVersionLabel(instance: MinecraftInstance) {
  return instance.modLoader.type === "fabric"
    ? `Minecraft ${instance.versionId} · Fabric ${instance.modLoader.version}`
    : `Minecraft ${instance.versionId}`;
}

export function useLauncher() {
  const [instances, setInstances] = useState<MinecraftInstance[]>([]);
  const [instancesLoading, setInstancesLoading] = useState(true);
  const [selectedId, setSelectedId] = useState("");
  const [instanceId, setInstanceId] = useState("minecraft");
  const [instanceName, setInstanceName] = useState("Minecraft");
  const [versionCatalog, setVersionCatalog] = useState<MinecraftVersionCatalog | null>(null);
  const [selectedVersionId, setSelectedVersionId] = useState("");
  const [versionQuery, setVersionQuery] = useState("");
  const [versionsLoading, setVersionsLoading] = useState(false);
  const [gameMode, setGameMode] = useState<"offline" | "demo">("offline");
  const [modLoaderType, setModLoaderType] = useState<"vanilla" | "fabric">("vanilla");
  const [fabricLoaders, setFabricLoaders] = useState<FabricLoaderVersion[]>([]);
  const [selectedFabricLoader, setSelectedFabricLoader] = useState("");
  const [fabricLoadersLoading, setFabricLoadersLoading] = useState(false);
  const [fabricLoadersError, setFabricLoadersError] = useState<string | null>(null);
  const [fabricReloadKey, setFabricReloadKey] = useState(0);
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [launchProgress, setLaunchProgress] = useState<MinecraftLaunchProgress | null>(null);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [runningIds, setRunningIds] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<
    "install" | "launch" | "stop" | "rename" | "delete" | "diagnose" | "repair" | null
  >(null);
  const [error, setError] = useState<string | null>(null);
  const [creatorError, setCreatorError] = useState<string | null>(null);
  const [versionsError, setVersionsError] = useState<string | null>(null);
  const [showCreator, setShowCreator] = useState(false);
  const [settingsName, setSettingsName] = useState("");
  const [authStatus, setAuthStatus] = useState<MicrosoftAuthStatus>({
    configured: false,
    authorized: false,
  });
  const [showAuth, setShowAuth] = useState(false);
  const [authChallenge, setAuthChallenge] = useState<MicrosoftSignInChallenge | null>(null);
  const [authBusy, setAuthBusy] = useState<"begin" | "signout" | null>(null);
  const [authError, setAuthError] = useState<string | null>(null);
  const [minecraftProfile, setMinecraftProfile] = useState<MinecraftAccountProfile | null>(null);
  const [minecraftProfileLoading, setMinecraftProfileLoading] = useState(false);
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
  const [diagnosis, setDiagnosis] = useState<InstanceDiagnosis | null>(null);
  const modSearchRequestRef = useRef(0);
  const installedModsRequestRef = useRef(0);
  const authRequestGenerationRef = useRef(0);
  const stoppingIdRef = useRef<string | null>(null);

  const selected = useMemo(
    () => instances.find((instance) => instance.id === selectedId) ?? null,
    [instances, selectedId],
  );
  const isRunning = selected ? runningIds.has(selected.id) : false;
  const visibleLogs = selected ? logs.filter((line) => line.instanceId === selected.id) : [];
  const installedProjectIds = useMemo(
    () => new Set(installedMods.map((item) => item.projectId)),
    [installedMods],
  );
  const modOperationActive = modInstallingProjectId !== null || modRemovingProjectId !== null;
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
  const progressPercent = progress
    ? progress.total === 0
      ? 0
      : Math.min(100, Math.max(0, Math.round((progress.completed / progress.total) * 100)))
    : 0;
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

  const openCreator = () => {
    setInstanceId(`instance-${crypto.randomUUID()}`);
    setCreatorError(null);
    setProgress(null);
    setShowCreator(true);
  };

  const openAuth = () => {
    setAuthError(null);
    setShowAuth(true);
  };

  const refreshInstalledMods = async (instanceId: string) => {
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
  };

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

  const refreshVersions = async () => {
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
  };

  const refreshMinecraftProfile = async () => {
    const generation = authRequestGenerationRef.current;
    setMinecraftProfileLoading(true);
    try {
      const profile = await invoke<MinecraftAccountProfile>("refresh_minecraft_account");
      if (authRequestGenerationRef.current !== generation) return;
      setMinecraftProfile(profile);
      setAuthError(null);
    } catch (cause) {
      if (authRequestGenerationRef.current !== generation) return;
      setMinecraftProfile(null);
      setAuthError(String(cause));
    } finally {
      if (authRequestGenerationRef.current === generation) setMinecraftProfileLoading(false);
    }
  };

  const refreshAuthStatus = async () => {
    const generation = authRequestGenerationRef.current;
    const status = await invoke<MicrosoftAuthStatus>("microsoft_auth_status");
    if (authRequestGenerationRef.current !== generation) return;
    setAuthStatus(status);
    if (status.authorized) void refreshMinecraftProfile();
  };

  useEffect(() => {
    if (!hasTauriRuntime()) {
      setInstancesLoading(false);
      return;
    }

    void refreshInstances().catch((cause) => setError(String(cause)));
    void refreshVersions().catch((cause) => setError(String(cause)));
    void refreshAuthStatus().catch((cause) => setAuthError(String(cause)));
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
    const unlistenModProgress = listen<ModInstallProgress>("modrinth-install-progress", (event) =>
      setModInstallProgress(event.payload),
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
      void unlistenModProgress.then((unlisten) => unlisten());
      void unlistenStatus.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    setDiagnosis(null);
    setError(null);
    setInstalledMods([]);
    setModError(null);
    setModSuccess(null);
    installedModsRequestRef.current += 1;
  }, [selectedId]);

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

  useEffect(() => {
    if (!authChallenge || authStatus.authorized) return;

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      try {
        const result = await invoke<MicrosoftSignInPoll>("poll_microsoft_sign_in", {
          sessionId: authChallenge.sessionId,
        });
        if (cancelled) return;
        if (result.status === "authorized") {
          authRequestGenerationRef.current += 1;
          setMinecraftProfileLoading(false);
          setMinecraftProfile(null);
          setAuthStatus((current) => ({ ...current, authorized: true }));
          setAuthChallenge(null);
          setAuthError(null);
          void refreshMinecraftProfile();
          return;
        }
        timer = setTimeout(poll, (result.retryAfter ?? authChallenge.interval) * 1000);
      } catch (cause) {
        if (!cancelled) {
          setAuthError(String(cause));
          setAuthChallenge(null);
        }
      }
    };

    timer = setTimeout(poll, authChallenge.interval * 1000);
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [authChallenge, authStatus.authorized]);

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

  const beginMicrosoftSignIn = async () => {
    setAuthBusy("begin");
    setAuthError(null);
    try {
      const challenge = await invoke<MicrosoftSignInChallenge>("begin_microsoft_sign_in");
      setAuthChallenge(challenge);
      await openMicrosoftVerification(challenge.verificationUri);
    } catch (cause) {
      setAuthError(String(cause));
    } finally {
      setAuthBusy(null);
    }
  };

  const openMicrosoftVerification = async (verificationUri: string) => {
    try {
      await openUrl(verificationUri);
    } catch (cause) {
      setAuthError(`ブラウザーを開けませんでした。下のURLを手動で開いてください: ${cause}`);
    }
  };

  const signOutMicrosoft = async () => {
    authRequestGenerationRef.current += 1;
    setMinecraftProfileLoading(false);
    setAuthBusy("signout");
    setAuthError(null);
    try {
      await invoke("sign_out_microsoft");
      setAuthChallenge(null);
      setMinecraftProfile(null);
      setAuthStatus((current) => ({ ...current, authorized: false }));
    } catch (cause) {
      setAuthError(String(cause));
    } finally {
      setAuthBusy(null);
    }
  };

  return {
    refreshInstances,
    instances,
    instancesLoading,
    setSelectedId,
    instanceName,
    setInstanceName,
    versionCatalog,
    selectedVersionId,
    setSelectedVersionId,
    versionQuery,
    setVersionQuery,
    versionsLoading,
    gameMode,
    setGameMode,
    modLoaderType,
    setModLoaderType,
    fabricLoaders,
    selectedFabricLoader,
    setSelectedFabricLoader,
    fabricLoadersLoading,
    fabricLoadersError,
    setFabricReloadKey,
    progress,
    launchProgress,
    runningIds,
    busy,
    error,
    setError,
    creatorError,
    versionsError,
    showCreator,
    setShowCreator,
    settingsName,
    setSettingsName,
    authStatus,
    showAuth,
    setShowAuth,
    authChallenge,
    authBusy,
    authError,
    minecraftProfile,
    minecraftProfileLoading,
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
    diagnosis,
    selected,
    isRunning,
    visibleLogs,
    installedProjectIds,
    modOperationActive,
    versionGroups,
    progressPercent,
    currentModInstallProgress,
    modInstallPercent,
    openCreator,
    openAuth,
    refreshInstalledMods,
    runModSearch,
    openMods,
    installModrinthMod,
    removeModrinthMod,
    refreshVersions,
    refreshMinecraftProfile,
    install,
    launch,
    stop,
    diagnoseSelected,
    repairSelected,
    renameSelected,
    deleteSelected,
    beginMicrosoftSignIn,
    openMicrosoftVerification,
    signOutMicrosoft,
  };
}

export type Launcher = ReturnType<typeof useLauncher>;
