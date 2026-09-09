import type { ReactNode } from "react";

export function ErrorMessage({ children }: { children?: ReactNode }) {
  return children ? (
    <div role="alert" className="error-message">
      {children}
    </div>
  ) : null;
}
