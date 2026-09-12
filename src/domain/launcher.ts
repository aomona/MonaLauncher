export type InstancePermissions = {
  gameWrite: boolean;
  narrator: boolean;
};

export type PermissionSupport = {
  platform: "windows" | "macos" | "unsupported";
  editable: boolean;
};

export type MinecraftInstance = {
  id: string;
  name: string;
  versionId: string;
  javaPath: string;
  gameDirectory: string;
  demo: boolean;
  sandboxed: boolean;
  permissions: InstancePermissions;
  modLoader:
    | {
        type: "vanilla";
      }
    | {
        type: "fabric";
        version: string;
      };
};

export type MinecraftVersion = {
  id: string;
  versionType: "release" | "snapshot" | "old_beta" | "old_alpha" | string;
  releaseTime: string;
};

export type MinecraftVersionCatalog = {
  latest: {
    release: string;
    snapshot: string;
  };
  versions: MinecraftVersion[];
};

export type FabricLoaderVersion = {
  version: string;
  stable: boolean;
};

export type InstallProgress = {
  stage: string;
  completed: number;
  total: number;
  message: string;
};

export type MinecraftLogEvent = {
  instanceId: string;
  stream: string;
  line: string;
};

export type MinecraftStatusEvent = {
  instanceId: string;
  status: "running" | "stopped";
  exitCode: number | null;
};

export type MinecraftLaunchProgress = {
  instanceId: string;
  stage: string;
  message: string;
};

export type MicrosoftAuthStatus = {
  configured: boolean;
  authorized: boolean;
};

export type MicrosoftSignInChallenge = {
  sessionId: string;
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
};

export type MicrosoftSignInPoll = {
  status: "pending" | "authorized";
  retryAfter: number | null;
};

export type MinecraftAccountProfile = {
  name: string;
  uuid: string;
};

export type ModSearchHit = {
  projectId: string;
  slug: string | null;
  title: string;
  description: string;
  author: string;
  downloads: number;
  follows: number;
  dateModified: string;
};

export type ModSearchResponse = {
  hits: ModSearchHit[];
  offset: number;
  limit: number;
  totalHits: number;
};

export type InstalledMod = {
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

export type ModInstallResult = {
  installed: InstalledMod[];
};

export type ModRemovalResult = {
  requested: InstalledMod;
  removed: InstalledMod[];
  retainedAsDependency: boolean;
  cleanupPending: boolean;
};

export type ModInstallProgress = {
  instanceId: string;
  completed: number;
  total: number;
  message: string;
};

export type DiagnosticCheck = {
  id: string;
  label: string;
  status: "ok" | "warning" | "error";
  detail: string;
  repairable: boolean;
};

export type InstanceDiagnosis = {
  instanceId: string;
  status: "healthy" | "repairable" | "attention";
  checkedFiles: number;
  issueCount: number;
  repairableCount: number;
  checks: DiagnosticCheck[];
};

export type LogLine = MinecraftLogEvent & { id: number };
