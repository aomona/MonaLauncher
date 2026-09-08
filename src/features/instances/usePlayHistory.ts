import { useEffect, useRef, useState } from "react";
import type { MinecraftInstance } from "../../domain/launcher";

function readHistory(): Record<string, number> {
  try {
    const value: unknown = JSON.parse(localStorage.getItem("mona:last-played") ?? "{}");
    return value && typeof value === "object" && !Array.isArray(value)
      ? Object.fromEntries(
          Object.entries(value).filter(
            ([, time]) => typeof time === "number" && Number.isFinite(time),
          ),
        )
      : {};
  } catch {
    return {};
  }
}

export function usePlayHistory(instances: MinecraftInstance[], runningIds: Set<string>) {
  const [history, setHistory] = useState(readHistory);
  const previousRunning = useRef(new Set<string>());
  useEffect(() => {
    const newlyRunning = [...runningIds].filter((id) => !previousRunning.current.has(id));
    previousRunning.current = new Set(runningIds);
    if (!newlyRunning.length) return;
    setHistory((current) => {
      const next = {
        ...current,
        ...Object.fromEntries(newlyRunning.map((id) => [id, Date.now()])),
      };
      try {
        localStorage.setItem("mona:last-played", JSON.stringify(next));
      } catch {
        /* In-memory history remains available. */
      }
      return next;
    });
  }, [runningIds]);

  const recent = instances
    .filter((item) => history[item.id])
    .sort((a, b) => history[b.id] - history[a.id]);
  const playing = instances
    .filter((item) => runningIds.has(item.id))
    .sort((a, b) => (history[b.id] ?? 0) - (history[a.id] ?? 0));

  return { history, recent, playing };
}
