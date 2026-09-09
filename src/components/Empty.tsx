import type { ReactNode } from "react";

export function Empty({
  title,
  children,
  action,
}: {
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <h2 className="text-section-title text-text-heading">{title}</h2>
      <div className="mt-2 max-w-home text-text-secondary">{children}</div>
      {action && <div className="mt-6 flex flex-wrap gap-2">{action}</div>}
    </div>
  );
}
