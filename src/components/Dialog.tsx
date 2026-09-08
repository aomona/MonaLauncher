/* Native dialog backdrop clicks complement onCancel (Escape); the dialog itself is not a button. */
/* eslint-disable jsx-a11y/no-noninteractive-element-interactions, jsx-a11y/click-events-have-key-events */
import { X } from "lucide-react";
import { useEffect, useId, useRef, type ReactNode } from "react";
import { Button } from "./Button";

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
