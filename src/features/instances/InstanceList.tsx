import { Box, MoreHorizontal, Play } from "lucide-react";
import type { Launcher } from "../../app/useLauncher";
import { Button } from "../../components/ui";
import type { MinecraftInstance } from "../../domain/launcher";

import { instanceVersionLabel } from "./instance-label";

type InstanceListModel = Pick<
  Launcher,
  "busy" | "launch" | "modOperationActive" | "runningIds" | "setSettingsName"
>;
export type OpenInstance = (instance: MinecraftInstance, tab?: string) => void;
export function InstanceList({
  items,
  launcher,
  openInstance,
}: {
  items: MinecraftInstance[];
  launcher: InstanceListModel;
  openInstance: OpenInstance;
}) {
  const { runningIds, busy } = launcher;
  return (
    <div aria-label="Minecraftインスタンス">
      {items.map((item) => (
        <div className="instance-row" key={item.id}>
          <span className="instance-avatar" aria-hidden="true">
            <Box size={20} />
          </span>
          <div className="min-w-0 flex-1">
            <button className="instance-name" title={item.name} onClick={() => openInstance(item)}>
              {item.name}
            </button>
            <p className="mt-1 truncate font-mono text-caption text-text-secondary">
              {instanceVersionLabel(item)}
            </p>
          </div>
          {runningIds.has(item.id) && (
            <span className="text-small text-success-foreground">Running</span>
          )}
          <div className="ml-auto flex shrink-0 items-center gap-2">
            <Button
              tone={runningIds.has(item.id) ? "secondary" : "primary"}
              disabled={Boolean(busy) || launcher.modOperationActive || !item.sandboxed}
              onClick={() => {
                if (runningIds.has(item.id)) openInstance(item);
                else {
                  launcher.setSettingsName(item.name);
                  void launcher.launch(item);
                }
              }}
            >
              {runningIds.has(item.id) ? (
                "Open instance"
              ) : (
                <>
                  <Play size={14} />
                  Play
                </>
              )}
            </Button>
            <details className="relative">
              <summary
                className="button button-ghost icon-button list-none"
                aria-label={`${item.name}の操作`}
              >
                <MoreHorizontal size={18} />
              </summary>
              <div className="absolute right-0 z-10 mt-1 min-w-(--mona-layout-sidebar-width) rounded-menu border border-border-subtle bg-background-floating p-1 shadow-floating">
                <Button
                  tone="ghost"
                  className="w-full justify-start"
                  onClick={(event) => {
                    event.currentTarget.closest("details")?.removeAttribute("open");
                    openInstance(item, "Settings");
                  }}
                >
                  インスタンス設定
                </Button>
              </div>
            </details>
          </div>
        </div>
      ))}
    </div>
  );
}
