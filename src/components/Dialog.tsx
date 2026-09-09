import { Dialog as BaseDialog } from "@base-ui/react/dialog";
import { X } from "lucide-react";
import type { ReactNode } from "react";
import { useRef } from "react";
import { Button } from "./Button";

export function Dialog({
  title,
  children,
  footer,
  onClose,
  finalFocus,
  large = false,
  className = "",
}: {
  title: string;
  children: ReactNode;
  footer?: ReactNode;
  onClose: () => void;
  finalFocus?: BaseDialog.Popup.Props["finalFocus"];
  large?: boolean;
  className?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  return (
    <BaseDialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <BaseDialog.Portal>
        <BaseDialog.Backdrop className="dialog-backdrop" />
        <BaseDialog.Viewport className="dialog-viewport">
          <BaseDialog.Popup
            ref={ref}
            finalFocus={finalFocus}
            className={`dialog ${large ? "dialog-large" : ""} ${className}`}
            initialFocus={() =>
              ref.current?.querySelector<HTMLElement>("[data-initial-focus]") ??
              ref.current?.querySelector<HTMLElement>("h2") ??
              true
            }
          >
            <header className="dialog-header">
              <BaseDialog.Title
                tabIndex={-1}
                className={large ? "text-page-title" : "text-section-title"}
              >
                {title}
              </BaseDialog.Title>
              <BaseDialog.Close
                render={
                  <Button tone="ghost" className="icon-button shrink-0" aria-label="閉じる" />
                }
              >
                <X size={18} aria-hidden="true" />
              </BaseDialog.Close>
            </header>
            {children}
            {footer && <footer className="dialog-footer">{footer}</footer>}
          </BaseDialog.Popup>
        </BaseDialog.Viewport>
      </BaseDialog.Portal>
    </BaseDialog.Root>
  );
}
