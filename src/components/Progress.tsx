export function Progress({ value, label }: { value?: number; label: string }) {
  return (
    <div className="min-w-0 flex-1">
      <output className="mb-2 flex flex-wrap justify-between gap-2 text-small">
        <span className="wrap-anywhere">{label}</span>
        {value !== undefined && <span className="tabular-nums">{value}%</span>}
      </output>
      <progress aria-label={label} max={100} value={value} className="progress" />
    </div>
  );
}
