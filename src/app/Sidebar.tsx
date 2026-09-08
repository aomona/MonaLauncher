import { Box, Download, Home, Images, Newspaper, Settings } from "lucide-react";
import { Button } from "../components/Button";
import type { MinecraftInstance } from "../domain/launcher";
import type { OpenInstance } from "../features/instances/InstanceList";
import type { Launcher } from "./useLauncher";

import { pages, type Page } from "./navigation";
const icons = { Home, Instances: Box, News: Newspaper, Gallery: Images, Settings };
export type SidebarProps = {
  page: Page;
  navigate: (page: Page) => void;
  launcher: Launcher;
  playing: MinecraftInstance[];
  recent: MinecraftInstance[];
  openInstance: OpenInstance;
};
export function Sidebar({ page, navigate, launcher, playing, recent, openInstance }: SidebarProps) {
  const { selected, busy, progress, launchProgress } = launcher;
  const operationLabel =
    launchProgress?.message ??
    progress?.message ??
    launcher.modInstallProgress?.message ??
    (busy === "stop"
      ? "Minecraftを終了しています…"
      : busy === "diagnose"
        ? "ファイルを検証しています…"
        : null);
  return (
    <aside className="sidebar" aria-label="アプリケーションナビゲーション">
      <nav aria-label="メインメニュー" className="min-h-0 flex-1 overflow-y-auto">
        <div className="flex flex-col gap-1">
          {pages.slice(0, 4).map((item) => {
            const Icon = icons[item];
            return (
              <button
                key={item}
                className="nav-item"
                aria-current={page === item ? "page" : undefined}
                onClick={() => navigate(item)}
              >
                <Icon size={18} strokeWidth={2} />
                <span>{item}</span>
              </button>
            );
          })}
        </div>
      </nav>
      <div className="mt-6 shrink-0">
        {operationLabel && (
          <div className="mb-6 px-3">
            <div className="mb-2 flex items-center gap-2 text-small">
              <Download size={16} />
              Activity
            </div>
            <output className="wrap-anywhere text-small text-text-secondary">
              {operationLabel}
            </output>
            {(selected || busy === "install") && (
              <Button
                tone="ghost"
                onClick={() => {
                  if (busy === "install") launcher.setShowCreator(true);
                  else if (selected) openInstance(selected);
                }}
              >
                処理の詳細
              </Button>
            )}
          </div>
        )}
        {(playing[0] || recent[0]) && (
          <button
            className="nav-item mb-6 flex-wrap text-left"
            onClick={() => openInstance(playing[0] ?? recent[0])}
            disabled={Boolean(
              (busy || launcher.modOperationActive) &&
              selected?.id !== (playing[0] ?? recent[0]).id,
            )}
          >
            <span className="w-full text-caption text-text-secondary">
              {playing.length ? "Now Playing" : "Last Played"}
            </span>
            <span className="min-w-0 flex-1 truncate">{(playing[0] ?? recent[0]).name}</span>
            {playing.length > 0 && (
              <span className="text-caption text-success-foreground">
                Running{playing.length > 1 ? ` · ほか${playing.length - 1}件` : ""}
              </span>
            )}
          </button>
        )}
        <div className="border-t border-border-subtle pt-3">
          <button
            className="nav-item"
            aria-current={page === "Settings" ? "page" : undefined}
            onClick={() => navigate("Settings")}
          >
            <Settings size={18} />
            Settings
          </button>
        </div>
      </div>
    </aside>
  );
}
