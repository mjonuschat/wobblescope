import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

/** A small uppercase label over a monospace value. */
export function Field({ label, className, children }: { label: string; className?: string; children: ReactNode }) {
  return (
    <div className={className}>
      <dt className="text-xs uppercase tracking-wide text-muted-foreground">{label}</dt>
      <dd className="font-mono text-sm">{children}</dd>
    </div>
  );
}

/** A grid of [`Field`]s. */
export function Fields({ className, children, ...props }: React.ComponentProps<"dl">) {
  return (
    <dl className={cn("grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-3 lg:grid-cols-4", className)} {...props}>
      {children}
    </dl>
  );
}
