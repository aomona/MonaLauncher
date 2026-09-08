import { Play } from "lucide-react";
import type { Launcher } from "../../../app/useLauncher";
import { Button, Progress } from "../../../components/ui";

type InstanceLaunchControlsModel = Pick<
  Launcher,
  | "busy"
  | "currentModInstallProgress"
  | "error"
  | "isRunning"
  | "launch"
  | "launchProgress"
  | "modInstallPercent"
  | "modOperationActive"
  | "progress"
  | "progressPercent"
  | "selected"
>;
export function InstanceLaunchControls({
  launcher: l,
  onForceQuit,
}: {
  launcher: InstanceLaunchControlsModel;
  onForceQuit: () => void;
}) {
  const instance = l.selected!;
  const label =
    l.launchProgress?.instanceId === instance.id
      ? l.launchProgress.message
      : (l.progress?.message ??
        l.currentModInstallProgress?.message ??
        (l.busy === "stop"
          ? "Stopping Minecraft…"
          : l.busy === "diagnose"
            ? "ファイルを検証しています…"
            : l.isRunning
              ? "Running"
              : ""));
  return (
    <>
      <output className="footer-status min-w-0 flex-1">
        {l.progress ? (
          <Progress label={label} value={l.progress.total > 0 ? l.progressPercent : undefined} />
        ) : l.currentModInstallProgress ? (
          <Progress
            label={label}
            value={l.currentModInstallProgress.total > 0 ? l.modInstallPercent : undefined}
          />
        ) : (
          <span className={l.isRunning ? "text-small text-success-foreground" : "text-small"}>
            {label}
          </span>
        )}
        {l.error && (
          <p className="text-small text-danger-foreground">
            処理に失敗しました。本文の詳細を確認してください。
          </p>
        )}
      </output>
      <Button
        tone={l.isRunning ? "danger-outline" : "primary"}
        aria-busy={l.busy === "launch" || l.busy === "stop"}
        disabled={Boolean(l.busy) || l.modOperationActive || !instance.sandboxed}
        onClick={() => {
          if (l.isRunning) onForceQuit();
          else void l.launch();
        }}
      >
        {l.busy === "launch" ? (
          "Starting…"
        ) : l.busy === "stop" ? (
          "Stopping…"
        ) : l.isRunning ? (
          "Force Quit"
        ) : (
          <>
            <Play size={14} />
            Play
          </>
        )}
      </Button>
    </>
  );
}
