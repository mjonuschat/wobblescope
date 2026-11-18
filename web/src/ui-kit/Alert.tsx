import type { ReactNode } from "react";

import { cn } from "@/lib/utils";
import { toneBox, toneIcon, toneText, type Tone } from "@/ui-kit/tone";

interface Props {
  tone: Tone;
  title?: string;
  /**
   * `box` sits inside a card, `inline` is one line of colored text with an icon, and `banner` spans
   * the page under the header.
   */
  variant?: "box" | "inline" | "banner";
  /** Defaults to `alert` for danger and `status` for the other tones. */
  role?: "alert" | "status";
  action?: ReactNode;
  className?: string;
  children?: ReactNode;
}

export function Alert({ tone, title, variant = "box", role, action, className, children }: Props) {
  const Icon = toneIcon[tone];
  const icon = <Icon aria-hidden="true" className={cn("mt-0.5 size-4 shrink-0", toneText[tone])} />;
  const body = (
    <div className="min-w-0 space-y-1">
      {title ? <p className="font-medium">{title}</p> : null}
      {children}
    </div>
  );
  return (
    <div
      role={role ?? (tone === "danger" ? "alert" : "status")}
      className={cn(
        "flex items-start gap-3 text-sm",
        variant === "box" && cn("rounded-lg border p-3", toneBox[tone]),
        variant === "banner" && cn("items-center border-b px-6 py-2", toneBox[tone]),
        variant === "inline" && toneText[tone],
        className,
      )}
    >
      {icon}
      {body}
      {action ? <div className="ml-auto shrink-0">{action}</div> : null}
    </div>
  );
}
