import { Progress as BaseProgress } from "@base-ui/react/progress";

export function Progress({ value, label }: { value?: number; label: string }) {
  return (
    <BaseProgress.Root value={value ?? null} className="min-w-0 flex-1">
      <div className="mb-2 flex flex-wrap justify-between gap-2 text-small">
        <BaseProgress.Label className="wrap-anywhere">{label}</BaseProgress.Label>
        {value !== undefined && <BaseProgress.Value className="tabular-nums" />}
      </div>
      <BaseProgress.Track className="progress">
        <BaseProgress.Indicator className="progress-indicator" />
      </BaseProgress.Track>
    </BaseProgress.Root>
  );
}
