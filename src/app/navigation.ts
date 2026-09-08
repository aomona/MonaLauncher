import { useEffect, useState } from "react";
import { hasTauriRuntime } from "../lib/tauri";

export const pages = ["Home", "Instances", "News", "Gallery", "Settings"] as const;
export type Page = (typeof pages)[number];
export function useNavigation() {
  const [page, setPage] = useState<Page>("Home");
  const [settingsTab, setSettingsTab] = useState("General");
  useEffect(() => {
    if (!hasTauriRuntime()) return;
    const keydown = (event: KeyboardEvent) => {
      if (
        event.isComposing ||
        !(event.metaKey || event.ctrlKey) ||
        document.querySelector("dialog[open]")
      )
        return;
      const next = event.key === "," ? "Settings" : pages[Number(event.key) - 1];
      if (next && (event.key === "," || /^[1-4]$/.test(event.key))) {
        event.preventDefault();
        setPage(next);
      }
    };
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  }, []);

  return {
    page,
    navigate: setPage,
    settingsTab,
    setSettingsTab,
    openAccountSettings: () => {
      setSettingsTab("General");
      setPage("Settings");
    },
  };
}
