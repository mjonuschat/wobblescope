import { OctagonXIcon, TriangleAlertIcon } from "lucide-react";
import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

const TONES = {
  warn: { box: "border-amber-800 bg-amber-950/40", icon: "text-amber-400", Icon: TriangleAlertIcon },
  error: { box: "border-red-900 bg-red-950/40", icon: "text-red-400", Icon: OctagonXIcon },
} as const;

/**
 * A boxed notice inside a card, with an icon, a title and one line per finding. A warning is
 * announced politely and an error as an alert.
 */
export function Callout({ tone, title, children }: { tone: keyof typeof TONES; title: string; children: ReactNode }) {
  const { box, icon, Icon } = TONES[tone];
  return (
    <div
      role={tone === "error" ? "alert" : "status"}
      className={cn("flex items-start gap-3 rounded-lg border p-3", box)}
    >
      <Icon aria-hidden="true" className={cn("mt-0.5 size-4 shrink-0", icon)} />
      <div className="min-w-0 space-y-1 text-sm">
        <p className="font-medium">{title}</p>
        {children}
      </div>
    </div>
  );
}
