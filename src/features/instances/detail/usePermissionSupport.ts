import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import type { PermissionSupport } from "../../../domain/launcher";
import { hasTauriRuntime } from "../../../lib/tauri";

export function usePermissionSupport() {
  const [permissionSupport, setPermissionSupport] = useState<PermissionSupport | null>(null);
  const [permissionSupportError, setPermissionSupportError] = useState<string | null>(null);
  const refreshPermissionSupport = useCallback(async () => {
    setPermissionSupportError(null);
    try {
      setPermissionSupport(await invoke<PermissionSupport>("minecraft_permission_support"));
    } catch (cause) {
      setPermissionSupportError(String(cause));
    }
  }, []);
  useEffect(() => {
    if (hasTauriRuntime()) void refreshPermissionSupport();
  }, [refreshPermissionSupport]);
  return { permissionSupport, permissionSupportError, refreshPermissionSupport };
}
