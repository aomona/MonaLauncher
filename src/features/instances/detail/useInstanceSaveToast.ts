import { useRef, useState } from "react";
import { createToastManager } from "../../../components/Toast";

/** Keep each save's pending and final feedback in the same notification. */
export function useInstanceSaveToast() {
  const [toasts] = useState(createToastManager);
  const failedToast = useRef<string | null>(null);
  const saveWithToast = async (save: () => Promise<boolean | void>, title: string) => {
    if (failedToast.current) toasts.close(failedToast.current);
    failedToast.current = null;
    const id = toasts.notify({ title: "保存中…", priority: "low", timeout: 0 });
    const ok = Boolean(await save());
    // add with the same ID also restores the result if the pending toast was dismissed.
    toasts.notify({
      id,
      title: ok ? title : "保存できませんでした",
      type: ok ? "success" : "error",
      priority: ok ? "low" : "high",
      timeout: ok ? 5000 : 0,
    });
    if (!ok) failedToast.current = id;
    return ok;
  };
  return { toasts, saveWithToast };
}
