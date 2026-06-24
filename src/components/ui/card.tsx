import type { ReactNode } from "react";
import { cn } from "../../lib/utils";

type CardProps = {
  title?: string;
  description?: string;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
};

export function Card({ title, description, actions, children, className }: CardProps) {
  return (
    <section className={cn("min-w-0 overflow-hidden rounded-lg border border-border bg-surface", className)}>
      {(title || description || actions) && (
        <header className="flex min-w-0 items-start justify-between gap-4 border-b border-border px-4 py-3">
          <div className="min-w-0">
            {title && <h2 className="text-sm font-semibold text-foreground break-words">{title}</h2>}
            {description && <p className="mt-1 text-xs leading-5 text-muted-foreground break-words">{description}</p>}
          </div>
          {actions && <div className="shrink-0">{actions}</div>}
        </header>
      )}
      <div className="min-w-0 p-4">{children}</div>
    </section>
  );
}
