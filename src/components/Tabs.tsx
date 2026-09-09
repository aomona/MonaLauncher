import { Tabs as BaseTabs } from "@base-ui/react/tabs";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { ComponentProps, ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { Button } from "./Button";

export function Tabs({
  id,
  tabs,
  active,
  onChange,
  children,
  panelProps,
}: {
  id: string;
  tabs: readonly string[];
  active: string;
  onChange: (tab: string) => void;
  children: ReactNode;
  panelProps?: Omit<ComponentProps<"div">, "id">;
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
    const observer = new ResizeObserver(() => {
      // Scroll buttons change the available width; keep the focused tab fully visible.
      el.querySelector<HTMLElement>(":focus")?.scrollIntoView({
        block: "nearest",
        inline: "nearest",
      });
      update();
    });
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
            onClick={() => ref.current?.scrollBy({ left: -200 })}
          >
            <ChevronLeft size={16} />
          </Button>
        )}
        <BaseTabs.List
          ref={ref}
          activateOnFocus
          aria-label={id === "instance" ? "インスタンスの詳細" : "設定の分類"}
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
            onClick={() => ref.current?.scrollBy({ left: 200 })}
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
