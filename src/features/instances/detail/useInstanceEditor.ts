import { useRef, useState } from "react";
import type { Launcher } from "../../../app/useLauncher";

/** Holds the draft's navigation guard across tab unmounts; failed saves never advance. */
export function useInstanceEditor(l: Launcher, save: () => Promise<boolean>) {
  const [guard, setGuard] = useState(false);
  const pending = useRef<(() => void) | null>(null);
  const draft = l.settingsName !== l.selected!.name;
  const request = (action: () => void) => {
    if (draft) {
      pending.current = action;
      setGuard(true);
    } else action();
  };
  const cancelNavigation = () => {
    setGuard(false);
    pending.current = null;
  };
  const complete = () => {
    const action = pending.current;
    pending.current = null;
    setGuard(false);
    action?.();
  };
  const discardAndContinue = () => {
    l.setSettingsName(l.selected!.name);
    complete();
  };
  const saveAndContinue = async () => {
    if (await save()) complete();
  };
  return {
    guard,
    draft,
    request,
    save,
    cancelNavigation,
    discardAndContinue,
    saveAndContinue,
  };
}
export type InstanceEditor = ReturnType<typeof useInstanceEditor>;
