import { Download } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { LogLine } from "../../../domain/launcher";
import { Button } from "../../../components/Button";
import { CopyButton } from "../../../components/CopyButton";

export function LogPanel({ entries }: { entries: LogLine[] }) {
  const [query, setQuery] = useState("");
  const [level, setLevel] = useState("all");
  const [follow, setFollow] = useState(true);
  const [wrap, setWrap] = useState(false);
  const output = useRef<HTMLDivElement>(null);
  const lines = entries.filter(
    (line) =>
      line.line.toLowerCase().includes(query.toLowerCase()) &&
      (level === "all" ||
        line.line.toUpperCase().includes(level) ||
        (level === "ERROR" && line.stream === "stderr")),
  );
  const text = lines.map((line) => `[${line.stream}] ${line.line}`).join("\n");
  useEffect(() => {
    if (follow && output.current && !window.getSelection()?.toString())
      output.current.scrollTop = output.current.scrollHeight;
  }, [follow, entries.length]);
  return (
    <>
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <label className="min-w-0 flex-1">
          <span className="sr-only">ログを検索</span>
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search logs…"
          />
        </label>
        <label>
          <span className="sr-only">ログレベル</span>
          <select value={level} onChange={(event) => setLevel(event.target.value)}>
            <option value="all">All levels</option>
            <option>DEBUG</option>
            <option>WARN</option>
            <option>ERROR</option>
          </select>
        </label>
        <CopyButton value={text} label="表示中のログ" />
        <Button
          onClick={() => {
            const url = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
            const anchor = document.createElement("a");
            anchor.href = url;
            anchor.download = "minecraft.log";
            anchor.click();
            setTimeout(() => URL.revokeObjectURL(url), 1000);
          }}
        >
          <Download size={16} />
          Export
        </Button>
      </div>
      <div className="mb-3 flex flex-wrap items-center gap-4">
        <label className="flex min-h-8 items-center gap-2">
          <input
            type="checkbox"
            checked={wrap}
            onChange={(event) => setWrap(event.target.checked)}
          />
          Wrap
        </label>
        <Button
          aria-pressed={follow}
          onClick={() => {
            setFollow(!follow);
            if (!follow && output.current) output.current.scrollTop = output.current.scrollHeight;
          }}
        >
          {follow ? "Follow: on" : "最新へ移動"}
        </Button>
      </div>
      <div
        ref={output}
        // The scrollable log must be keyboard focusable for local horizontal scrolling.
        // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
        tabIndex={0}
        role="log"
        aria-live="off"
        aria-label="Minecraftログ"
        className={`log-lines ${wrap ? "log-wrap" : ""}`}
        onScroll={(event) => {
          const el = event.currentTarget;
          if (el.scrollHeight - el.scrollTop - el.clientHeight > 32) setFollow(false);
        }}
      >
        {!lines.length ? (
          <p className="text-log-debug">
            {entries.length
              ? "条件に一致するログはありません。"
              : "起動すると、このインスタンスのログが表示されます。"}
          </p>
        ) : (
          lines.map((line) => (
            <div
              key={line.id}
              className={
                line.stream === "stderr" || /ERROR/.test(line.line)
                  ? "text-log-error"
                  : /WARN/.test(line.line)
                    ? "text-log-warning"
                    : /DEBUG/.test(line.line)
                      ? "text-log-debug"
                      : ""
              }
            >
              <code>
                [{line.stream}] {line.line}
              </code>
            </div>
          ))
        )}
      </div>
    </>
  );
}
