import { Menu } from "lucide-react";
import { useState, type ReactNode } from "react";
import { Button, Dialog } from "../components/ui";
import { Sidebar, type SidebarProps } from "./Sidebar";

export function AppShell({ children, ...navigation }: SidebarProps & { children: ReactNode }) {
  const [drawer, setDrawer] = useState(false);
  const sidebar = (
    <Sidebar
      {...navigation}
      navigate={(page) => {
        navigation.navigate(page);
        setDrawer(false);
      }}
      openInstance={(instance, tab) => {
        navigation.openInstance(instance, tab);
        setDrawer(false);
      }}
    />
  );
  return (
    <div className="design-surface flex h-dvh overflow-hidden">
      <div className="hidden shell:block">{sidebar}</div>
      <main className="min-h-0 min-w-0 flex-1 overflow-y-auto p-4 shell:p-8">
        <div className="mb-4 shell:hidden">
          <Button tone="ghost" aria-label="ナビゲーションを開く" onClick={() => setDrawer(true)}>
            <Menu size={18} />
            Menu
          </Button>
        </div>
        {children}
      </main>
      {drawer && (
        <Dialog title="Navigation" onClose={() => setDrawer(false)} className="drawer">
          {sidebar}
        </Dialog>
      )}
    </div>
  );
}
