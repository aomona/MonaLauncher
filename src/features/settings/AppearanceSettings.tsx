import { useEffect, useState } from "react";
import { ErrorMessage } from "../../components/ui";
import { readTheme } from "./appearance";
export function AppearanceSettings() {
  const [theme, setTheme] = useState(readTheme);
  const [preferenceError, setPreferenceError] = useState("");
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
  return (
    <>
      <h2 className="text-section-title">Appearance</h2>
      <div className="setting-row">
        <div>
          <label htmlFor="theme" className="text-navigation">
            Theme
          </label>
          <p className="mt-1 text-small text-text-secondary">
            アプリの外観。SystemはOSの設定に従います。
          </p>
        </div>
        <select
          id="theme"
          className="max-w-[12.5rem]"
          value={theme}
          onChange={(event) => {
            const next = event.target.value;
            try {
              localStorage.setItem("mona:theme", next);
              setTheme(next);
              setPreferenceError("");
            } catch {
              setPreferenceError("外観を保存できませんでした。もう一度お試しください。");
            }
          }}
        >
          <option value="system">System</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </div>
      <ErrorMessage>{preferenceError}</ErrorMessage>
    </>
  );
}
