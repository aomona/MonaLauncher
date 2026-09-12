import { Toast } from "@base-ui/react/toast";
import { Check, CircleAlert, Info, TriangleAlert, X } from "lucide-react";
import { Button } from "./Button";

export const ToastProvider = Toast.Provider;
export type ToastTone = "neutral" | "success" | "error" | "warning";
export function createToastManager() {
  const manager = Toast.createToastManager();
  return {
    ...manager,
    notify: (options: Omit<Parameters<typeof manager.add>[0], "type"> & { type?: ToastTone }) =>
      manager.add({ type: "neutral", ...options }),
  };
}
const toneIcons = { neutral: Info, success: Check, error: CircleAlert, warning: TriangleAlert };

/** Render inside the active dialog so its controls belong to the same focus boundary. */
export function ToastViewport() {
  const { toasts } = Toast.useToastManager();
  return (
    <Toast.Viewport className="toast-viewport" aria-label="通知">
      {toasts.map((toast) => {
        const Icon = toneIcons[toast.type as ToastTone] ?? Info;
        return (
          <Toast.Root key={toast.id} toast={toast} className="toast">
            <Icon className="shrink-0" size={16} aria-hidden="true" />
            <Toast.Content className="min-w-0 flex-1">
              <Toast.Title className="text-small wrap-anywhere" />
            </Toast.Content>
            <Toast.Close
              render={
                <Button tone="ghost" className="toast-close shrink-0" aria-label="通知を閉じる" />
              }
            >
              <X size={16} aria-hidden="true" />
            </Toast.Close>
          </Toast.Root>
        );
      })}
    </Toast.Viewport>
  );
}
