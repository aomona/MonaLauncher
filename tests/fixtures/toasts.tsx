import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { createToastManager, ToastProvider, ToastViewport } from "../../src/components/Toast";

/** Browser-only fixture; never imported by the application. */
export function mountToasts(limit = 4) {
  const container = document.createElement("div");
  document.body.append(container);
  const manager = createToastManager();
  flushSync(() =>
    createRoot(container).render(
      <ToastProvider toastManager={manager} timeout={0} limit={limit}>
        <ToastViewport />
      </ToastProvider>,
    ),
  );
  return () => {
    manager.notify({ title: "通常の通知" });
    manager.notify({ title: "保存しました", type: "success" });
    manager.notify({ title: "保存できませんでした", type: "error" });
    manager.notify({ title: "接続を確認してください", type: "warning" });
  };
}
