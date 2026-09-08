import { Plus } from "lucide-react";
import { useEffect } from "react";
import type { Launcher } from "../../app/useLauncher";
import { Button } from "../../components/ui";

import { InstalledMods } from "./InstalledMods";
export function ModsPanel({ launcher: l }: { launcher: Launcher }) {
  const instance = l.selected!;
  const { refreshInstalledMods, setModError } = l;
  useEffect(() => {
    void refreshInstalledMods(instance.id).catch((cause) => setModError(String(cause)));
  }, [instance.id, refreshInstalledMods, setModError]);
  return (
    <>
      <div className="mb-4 flex flex-wrap items-center justify-between gap-4">
        <h3 className="text-section-title">Installed Mods</h3>
        <Button
          disabled={
            l.isRunning ||
            Boolean(l.busy) ||
            l.modOperationActive ||
            instance.modLoader.type !== "fabric"
          }
          onClick={l.openMods}
        >
          <Plus size={16} />
          Add mods
        </Button>
      </div>
      {l.isRunning && (
        <p className="mb-4 text-warning-foreground">
          実行中のファイル変更はできません。Minecraftを終了してください。
        </p>
      )}
      {instance.modLoader.type !== "fabric" && (
        <p className="mb-4 text-text-secondary">
          Modrinthからの導入はFabricインスタンスで利用できます。
        </p>
      )}
      <InstalledMods launcher={l} />
    </>
  );
}
