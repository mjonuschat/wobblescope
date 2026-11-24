import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

interface Props {
  title: string;
  /** Set for an identifier such as a session id. */
  mono?: boolean;
  leading?: ReactNode;
  actions?: ReactNode;
}

export function PageHeader({ title, mono, leading, actions }: Props) {
  return (
    <div className="flex items-center gap-3">
      {leading}
      <h1 className={cn("min-w-0 truncate text-[28px] leading-[1.15] font-medium", mono && "font-mono text-2xl")}>
        {title}
      </h1>
      {actions ? <div className="ml-auto flex items-center gap-2">{actions}</div> : null}
    </div>
  );
}
