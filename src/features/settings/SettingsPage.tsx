import type { Launcher } from "../../app/useLauncher";
import { Empty } from "../../components/Empty";
import { Tabs } from "../../components/Tabs";

import { AccountSettings } from "../auth/AccountSettings";
import { AppearanceSettings } from "./AppearanceSettings";
export function SettingsPage({
  launcher,
  settingsTab,
  setSettingsTab,
}: {
  launcher: Launcher;
  settingsTab: string;
  setSettingsTab: (tab: string) => void;
}) {
  return (
    <>
      <Tabs
        id="settings"
        tabs={["General", "Minecraft", "Java", "Advanced"]}
        active={settingsTab}
        onChange={setSettingsTab}
        panelProps={{ className: "pt-6" }}
      >
        {settingsTab === "General" ? (
          <>
            <AppearanceSettings />
            <h2 className="mt-8 text-section-title">Language</h2>
            <div className="setting-row">
              <div>
                <p className="text-navigation">表示言語</p>
                <p className="mt-1 text-small text-text-secondary">
                  現在は日本語の説明と英語のナビゲーションに対応しています。
                </p>
              </div>
              <span>日本語</span>
            </div>
            <AccountSettings launcher={launcher} />
          </>
        ) : (
          <Empty title={`${settingsTab} settings`}>
            <p>
              この分類のグローバル設定はまだ変更できません。インスタンス固有の値は、各インスタンスの詳細で確認できます。
            </p>
          </Empty>
        )}
      </Tabs>
    </>
  );
}
