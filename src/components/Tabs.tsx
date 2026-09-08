import { ChevronLeft, ChevronRight } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "./Button";

export function Tabs({
  id,
  tabs,
  active,
  onChange,
}: {
  id: string;
  tabs: readonly string[];
  active: string;
  onChange: (tab: string) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [edges, setEdges] = useState({ left: false, right: false });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () =>
      setEdges({
        left: el.scrollLeft > 1,
        right: el.scrollLeft + el.clientWidth < el.scrollWidth - 1,
      });
    const observer = new ResizeObserver(update);
    observer.observe(el);
    el.addEventListener("scroll", update);
    update();
    return () => {
      observer.disconnect();
      el.removeEventListener("scroll", update);
    };
  }, []);
  useEffect(() => {
    ref.current
      ?.querySelector<HTMLElement>('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [active]);
  return (
    <div className="tabs-wrap">
      {edges.left && (
        <Button
          tone="ghost"
          className="tab-scroll"
          aria-label="前のタブを表示"
          onClick={() => ref.current?.scrollBy({ left: -200 })}
        >
          <ChevronLeft size={16} />
        </Button>
      )}
      <div
        ref={ref}
        role="tablist"
        aria-label={id === "instance" ? "インスタンスの詳細" : "設定の分類"}
        className="tabs"
      >
        {tabs.map((tab, index) => (
          <button
            key={tab}
            id={`${id}-tab-${index}`}
            role="tab"
            aria-selected={tab === active}
            aria-controls={`${id}-panel`}
            tabIndex={tab === active ? 0 : -1}
            className="tab"
            onClick={() => onChange(tab)}
            onKeyDown={(event) => {
              if (event.nativeEvent.isComposing) return;
              const next =
                event.key === "ArrowRight"
                  ? (index + 1) % tabs.length
                  : event.key === "ArrowLeft"
                    ? (index - 1 + tabs.length) % tabs.length
                    : event.key === "Home"
                      ? 0
                      : event.key === "End"
                        ? tabs.length - 1
                        : -1;
              if (next >= 0) {
                event.preventDefault();
                onChange(tabs[next]);
                document.getElementById(`${id}-tab-${next}`)?.focus();
              }
            }}
          >
            {tab}
          </button>
        ))}
      </div>
      {edges.right && (
        <Button
          tone="ghost"
          className="tab-scroll"
          aria-label="次のタブを表示"
          onClick={() => ref.current?.scrollBy({ left: 200 })}
        >
          <ChevronRight size={16} />
        </Button>
      )}
    </div>
  );
}
