import { Check, Copy } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "./Button";

export function CopyButton({ value, label }: { value: string; label: string }) {
  const [message, setMessage] = useState("");
  useEffect(() => {
    if (!message) return;
    const timer = setTimeout(() => setMessage(""), 2500);
    return () => clearTimeout(timer);
  }, [message]);
  return (
    <span className="inline-flex items-center gap-2">
      <Button
        tone="ghost"
        className="icon-button"
        aria-label={`${label}をコピー`}
        onClick={() => {
          void navigator.clipboard.writeText(value).then(
            () => setMessage("コピーしました"),
            () => setMessage("コピーできませんでした"),
          );
        }}
      >
        {message === "コピーしました" ? <Check size={16} /> : <Copy size={16} />}
      </Button>
      <output className="text-small">{message}</output>
    </span>
  );
}
