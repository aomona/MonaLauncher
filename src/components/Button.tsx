import { Button as BaseButton } from "@base-ui/react/button";
import { type ComponentProps } from "react";

export function Button({
  tone = "secondary",
  className = "",
  ...props
}: ComponentProps<"button"> & {
  tone?: "primary" | "secondary" | "ghost" | "danger" | "danger-outline";
}) {
  return <BaseButton type="button" className={`button button-${tone} ${className}`} {...props} />;
}
