import { type ButtonHTMLAttributes } from "react";

export function Button({
  tone = "secondary",
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  tone?: "primary" | "secondary" | "ghost" | "danger" | "danger-outline";
}) {
  return <button type="button" className={`button button-${tone} ${className}`} {...props} />;
}
