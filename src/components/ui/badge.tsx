import type { ReactNode } from "react";
import { cn } from "../../lib/utils";

type BadgeProps = {
  children: ReactNode;
  tone?: "neutral" | "success" | "warning" | "danger";
};

const tones = {
  neutral: "border-border bg-muted text-muted-foreground",
  success: "border-success/30 bg-success/10 text-success",
  warning: "border-warning/30 bg-warning/10 text-warning",
  danger: "border-danger/30 bg-danger/10 text-danger",
};

export function Badge({ children, tone = "neutral" }: BadgeProps) {
  return (
    <span className={cn("inline-flex max-w-full items-center rounded border px-2 py-0.5 text-xs font-medium break-words", tones[tone])}>
      {children}
    </span>
  );
}
