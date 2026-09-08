/* Native dialog backdrop clicks complement onCancel (Escape); the dialog itself is not a button. */
/* eslint-disable jsx-a11y/no-noninteractive-element-interactions, jsx-a11y/click-events-have-key-events */
import { Check, ChevronLeft, ChevronRight, Copy, X } from "lucide-react";
import {
  useEffect,
  useId,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type ReactNode,
} from "react";

export function Button({
  tone = "secondary",
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  tone?: "primary" | "secondary" | "ghost" | "danger" | "danger-outline";
}) {
  return <button type="button" className={`button button-${tone} ${className}`} {...props} />;
}
export function Empty({
  title,
  children,
  action,
}: {
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <h2 className="text-section-title text-text-heading">{title}</h2>
      <div className="mt-2 max-w-home text-text-secondary">{children}</div>
      {action && <div className="mt-6 flex flex-wrap gap-2">{action}</div>}
    </div>
  );
}
export function ErrorMessage({ children }: { children?: ReactNode }) {
  return children ? (
    <div role="alert" className="error-message">
      {children}
    </div>
  ) : null;
}
export function Dialog({
  title,
  children,
  footer,
  onClose,
  large = false,
  className = "",
}: {
  title: string;
  children: ReactNode;
  footer?: ReactNode;
  onClose: () => void;
  large?: boolean;
  className?: string;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const startedOutside = useRef(false);
  const titleId = useId();
  useEffect(() => {
    const element = ref.current;
    const previous = document.activeElement;
    element?.showModal();
    (
      element?.querySelector<HTMLElement>("[data-initial-focus]") ??
      element?.querySelector<HTMLElement>("h2")
    )?.focus();
    return () => {
      element?.close();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, []);
  const outside = (x: number, y: number) => {
    const box = ref.current?.getBoundingClientRect();
    return box ? x < box.left || x > box.right || y < box.top || y > box.bottom : false;
  };
  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      className={`dialog ${large ? "dialog-large" : ""} ${className}`}
      onCancel={(event) => {
        event.preventDefault();
        event.stopPropagation();
        onClose();
      }}
      onPointerDown={(event) => {
        startedOutside.current =
          event.target === event.currentTarget && outside(event.clientX, event.clientY);
      }}
      onClick={(event) => {
        if (
          startedOutside.current &&
          event.target === event.currentTarget &&
          outside(event.clientX, event.clientY)
        ) {
          event.stopPropagation();
          onClose();
        }
        startedOutside.current = false;
      }}
    >
      <header className="dialog-header">
        <h2 id={titleId} tabIndex={-1} className={large ? "text-page-title" : "text-section-title"}>
          {title}
        </h2>
        <Button tone="ghost" className="icon-button shrink-0" aria-label="閉じる" onClick={onClose}>
          <X size={18} />
        </Button>
      </header>
      {children}
      {footer && <footer className="dialog-footer">{footer}</footer>}
    </dialog>
  );
}
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
export function CopyButton({ value, label }: { value: string; label: string }) {
  const [message, setMessage] = useState("");
  useEffect(() => {
    if (!message) return;
    const timer = setTimeout(() => setMessage(""), 2500);
    return () => clearTimeout(timer);
  }, [message]);
  return (
    <span className="inline-flex items-center gap-2">
      <Button
        tone="ghost"
        className="icon-button"
        aria-label={`${label}をコピー`}
        onClick={() => {
          void navigator.clipboard.writeText(value).then(
            () => setMessage("コピーしました"),
            () => setMessage("コピーできませんでした"),
          );
        }}
      >
        {message === "コピーしました" ? <Check size={16} /> : <Copy size={16} />}
      </Button>
      <output className="text-small">{message}</output>
    </span>
  );
}
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
