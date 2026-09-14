import { Menu as BaseMenu } from "@base-ui/react/menu";
import type { ReactElement, ReactNode } from "react";

export function Menu({ trigger, children }: { trigger: ReactElement; children: ReactNode }) {
  return (
    <BaseMenu.Root>
      <BaseMenu.Trigger render={trigger} />
      <BaseMenu.Portal>
        <BaseMenu.Positioner align="end" sideOffset={4} className="z-50">
          <BaseMenu.Popup className="design-surface min-w-(--mona-layout-sidebar-width) max-w-(--available-width) rounded-menu border border-border-subtle bg-background-floating p-1 shadow-floating outline-none">
            {children}
          </BaseMenu.Popup>
        </BaseMenu.Positioner>
      </BaseMenu.Portal>
    </BaseMenu.Root>
  );
}

export function MenuItem(props: BaseMenu.Item.Props) {
  return (
    <BaseMenu.Item
      className="flex min-h-(--mona-controls-menu-item-min-height) cursor-pointer items-center rounded-control px-3 py-2 text-small outline-none data-highlighted:bg-navigation-hover data-disabled:cursor-not-allowed data-disabled:text-disabled-foreground"
      {...props}
    />
  );
}
