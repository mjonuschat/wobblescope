import type { ReactNode } from "react";

import { cn } from "@/lib/utils";
import { toneDot, type Tone } from "@/ui-kit/tone";

export function StatusBadge({ tone, children }: { tone?: Tone; children: ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
      <span
        aria-hidden="true"
        className={cn("size-1.5 shrink-0 rounded-full", tone ? toneDot[tone] : "bg-muted-foreground/50")}
      />
      {children}
    </span>
  );
}
