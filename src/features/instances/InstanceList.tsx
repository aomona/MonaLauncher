import { Box, MoreHorizontal, Pencil, Play } from "lucide-react";
import { useRef, useState } from "react";
import type { Launcher } from "../../app/useLauncher";
import { Button } from "../../components/Button";
import { Menu, MenuItem } from "../../components/Menu";
import type { MinecraftInstance } from "../../domain/launcher";

import { instanceVersionLabel } from "./instance-label";
import { OfflineLaunchDialog } from "./OfflineLaunchDialog";

type InstanceListModel = Pick<
  Launcher,
  "busy" | "launch" | "duplicate" | "modOperationActive" | "runningIds" | "setSettingsName"
>;
export type OpenInstance = (instance: MinecraftInstance, tab?: string, action?: "delete") => void;
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
  const [offlineInstance, setOfflineInstance] = useState<MinecraftInstance | null>(null);
  const menuTriggers = useRef(new Map<string, HTMLButtonElement>());
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
            <Button
              tone="ghost"
              className="icon-button"
              aria-label={`${item.name}を編集`}
              title="Edit"
              onClick={() => openInstance(item, "Overview")}
            >
              <Pencil size={16} aria-hidden="true" />
            </Button>
            <Menu
              trigger={
                <Button
                  tone="ghost"
                  className="icon-button"
                  aria-label={`${item.name}の操作`}
                  ref={(node) => {
                    if (node) menuTriggers.current.set(item.id, node);
                    else menuTriggers.current.delete(item.id);
                  }}
                >
                  <MoreHorizontal size={18} aria-hidden="true" />
                </Button>
              }
            >
              <MenuItem
                disabled={
                  Boolean(busy) ||
                  launcher.modOperationActive ||
                  runningIds.has(item.id) ||
                  !item.sandboxed
                }
                onClick={() => {
                  setOfflineInstance(item);
                }}
              >
                オフラインモードで起動
              </MenuItem>
              <MenuItem
                disabled={
                  Boolean(busy) ||
                  launcher.modOperationActive ||
                  runningIds.has(item.id) ||
                  !item.sandboxed
                }
                onClick={() => {
                  launcher.setSettingsName(item.name);
                  void launcher.launch(item, "demo");
                }}
              >
                デモモードで起動
              </MenuItem>
              <MenuItem
                disabled={
                  Boolean(busy) ||
                  launcher.modOperationActive ||
                  runningIds.has(item.id) ||
                  !item.sandboxed
                }
                onClick={() => {
                  void launcher.duplicate(item).then((copied) => {
                    if (copied) openInstance(copied, "Overview");
                  });
                }}
              >
                複製
              </MenuItem>
              <MenuItem
                disabled={Boolean(busy) || launcher.modOperationActive || runningIds.has(item.id)}
                onClick={() => openInstance(item, "Settings", "delete")}
              >
                削除
              </MenuItem>
            </Menu>
          </div>
        </div>
      ))}
      {offlineInstance && (
        <OfflineLaunchDialog
          instance={offlineInstance}
          disabled={
            Boolean(busy) || launcher.modOperationActive || runningIds.has(offlineInstance.id)
          }
          finalFocus={() => menuTriggers.current.get(offlineInstance.id) ?? null}
          onClose={() => setOfflineInstance(null)}
          onLaunch={(username) => {
            launcher.setSettingsName(offlineInstance.name);
            void launcher.launch(offlineInstance, "offline", username);
            setOfflineInstance(null);
          }}
        />
      )}
    </div>
  );
}
