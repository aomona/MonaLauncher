import { RefreshCw } from "lucide-react";
import type { Launcher } from "../../../app/useLauncher";
import { Button } from "../../../components/ui";

type InstanceDiagnosticsModel = Pick<
  Launcher,
  "busy" | "diagnoseSelected" | "diagnosis" | "isRunning" | "selected"
>;
export function InstanceDiagnostics({
  launcher: l,
  onRepair,
}: {
  launcher: InstanceDiagnosticsModel;
  onRepair: () => void;
}) {
  const instance = l.selected!;
  return (
    <section className="mt-8">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <h3 className="text-section-title">インスタンスの診断</h3>
        <Button disabled={Boolean(l.busy)} onClick={() => void l.diagnoseSelected()}>
          <RefreshCw size={16} />
          {l.busy === "diagnose" ? "診断中…" : "診断する"}
        </Button>
      </div>
      <p className="mt-2 text-text-secondary">管理対象ファイルと隔離設定を確認します。</p>
      {l.diagnosis?.instanceId === instance.id && (
        <div className="mt-4">
          <p className="text-small">
            {l.diagnosis.checkedFiles}ファイル検証 · {l.diagnosis.issueCount}件の問題
          </p>
          <ul>
            {l.diagnosis.checks.map((check) => (
              <li className="border-b border-border-subtle py-4" key={check.id}>
                <p
                  className={
                    check.status === "ok"
                      ? "text-success-foreground"
                      : check.status === "error"
                        ? "text-danger-foreground"
                        : "text-warning-foreground"
                  }
                >
                  {check.status === "ok" ? "正常" : check.status === "error" ? "Error" : "Warning"}{" "}
                  · {check.label}
                </p>
                <p className="mt-1 wrap-anywhere text-small text-text-secondary">{check.detail}</p>
              </li>
            ))}
          </ul>
          {l.diagnosis.repairableCount > 0 && (
            <Button className="mt-4" disabled={Boolean(l.busy) || l.isRunning} onClick={onRepair}>
              管理対象ファイルを修復
            </Button>
          )}
        </div>
      )}
    </section>
  );
}
