import { Tabs as BaseTabs } from "@base-ui/react/tabs";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { ComponentProps, ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { Button } from "./Button";

function revealTab(list: HTMLElement, tab: HTMLElement | null) {
  if (!tab) return;
  const listRect = list.getBoundingClientRect();
  const tabRect = tab.getBoundingClientRect();
  const left = tabRect.left - (listRect.left + list.clientLeft);
  const right = tabRect.right - (listRect.left + list.clientLeft + list.clientWidth);
  // A tab wider than the viewport is already as visible as possible if it spans both edges.
  if (left < 0 && right > 0) return;
  const delta = left < 0 ? left : right > 0 ? right : 0;
  if (delta) list.scrollBy({ left: delta, behavior: "instant" });
}

export function Tabs({
  id,
  label,
  tabs,
  active,
  onChange,
  children,
  panelProps,
}: {
  id: string;
  label?: string;
  tabs: readonly string[];
  active: string;
  onChange: (tab: string) => void;
  children: ReactNode;
  panelProps?: Omit<ComponentProps<"div">, "id">;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const manualScroll = useRef(false);
  const [edges, setEdges] = useState({ left: false, right: false });
  // Inline arrays from callers should not restart observers unless their labels change.
  const tabsKey = JSON.stringify(tabs);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => {
      const left = el.scrollLeft > 1;
      const right = el.scrollLeft + el.clientWidth < el.scrollWidth - 1;
      setEdges((previous) =>
        previous.left === left && previous.right === right ? previous : { left, right },
      );
    };
    const observer = new ResizeObserver(() => {
      // Scroll buttons change the available width; keep the focused tab fully visible.
      if (!manualScroll.current)
        revealTab(
          el,
          el.querySelector<HTMLElement>(":focus") ??
            el.querySelector<HTMLElement>('[aria-selected="true"]'),
        );
      update();
    });
    observer.observe(el);
    // Content can grow (e.g. fonts or labels) without changing the list's own width.
    for (const tab of el.querySelectorAll<HTMLElement>('[role="tab"]')) observer.observe(tab);
    el.addEventListener("scroll", update);
    update();
    return () => {
      observer.disconnect();
      el.removeEventListener("scroll", update);
    };
  }, [tabsKey]);
  useEffect(() => {
    manualScroll.current = false;
    const el = ref.current;
    if (el) revealTab(el, el.querySelector<HTMLElement>('[aria-selected="true"]'));
  }, [active]);
  return (
    <BaseTabs.Root
      value={active}
      onValueChange={(value) => {
        if (typeof value === "string") onChange(value);
      }}
      className="flex min-h-0 flex-1 flex-col"
    >
      <div className="tabs-wrap">
        {edges.left && (
          <Button
            tone="ghost"
            className="tab-scroll"
            aria-label="前のタブを表示"
            onClick={() => {
              manualScroll.current = true;
              ref.current?.scrollBy({ left: -200 });
            }}
          >
            <ChevronLeft size={16} />
          </Button>
        )}
        <BaseTabs.List
          ref={ref}
          onFocusCapture={() => {
            manualScroll.current = false;
          }}
          activateOnFocus
          aria-label={label ?? (id === "instance" ? "インスタンスの詳細" : "設定の分類")}
          className="tabs"
        >
          {tabs.map((tab, index) => (
            <BaseTabs.Tab key={tab} id={`${id}-tab-${index}`} value={tab} className="tab">
              {tab}
            </BaseTabs.Tab>
          ))}
        </BaseTabs.List>
        {edges.right && (
          <Button
            tone="ghost"
            className="tab-scroll"
            aria-label="次のタブを表示"
            onClick={() => {
              manualScroll.current = true;
              ref.current?.scrollBy({ left: 200 });
            }}
          >
            <ChevronRight size={16} />
          </Button>
        )}
      </div>
      <BaseTabs.Panel value={active} {...panelProps}>
        {children}
      </BaseTabs.Panel>
    </BaseTabs.Root>
  );
}
