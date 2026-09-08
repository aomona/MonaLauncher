import { useRef, useState } from "react";
import type { Launcher } from "../../app/useLauncher";
import type { MinecraftInstance } from "../../domain/launcher";

/** Owns instance-dialog identity and per-instance navigation memory across pages. */
export function useInstanceNavigation(launcher: Launcher) {
  const [instanceOpen, setInstanceOpen] = useState(false);
  const [initialTab, setInitialTab] = useState("Overview");
  const tabMemory = useRef<Record<string, string>>({});
  const scrollMemory = useRef<Record<string, number>>({});
  const openInstance = (instance: MinecraftInstance, tab?: string) => {
    if ((launcher.busy || launcher.modOperationActive) && launcher.selected?.id !== instance.id)
      return;
    launcher.setSelectedId(instance.id);
    launcher.setSettingsName(instance.name);
    setInitialTab(tab ?? tabMemory.current[instance.id] ?? "Overview");
    setInstanceOpen(true);
  };
  const onCreated = (instance: MinecraftInstance) => {
    setInitialTab("Overview");
    launcher.setSettingsName(instance.name);
    setInstanceOpen(true);
  };
  return {
    instanceOpen,
    initialTab,
    scrollMemory: scrollMemory.current,
    openInstance,
    onCreated,
    closeInstance: () => setInstanceOpen(false),
    rememberTab: (tab: string) => {
      if (launcher.selected) tabMemory.current[launcher.selected.id] = tab;
    },
  };
}
