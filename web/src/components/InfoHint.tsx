import { InfoIcon } from "lucide-react";
import type { ReactNode } from "react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

export function InfoHint({ label, children }: { label: string; children: ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            aria-label={`About ${label.toLowerCase()}`}
            className="flex cursor-help items-center rounded p-1 text-muted-foreground hover:text-foreground focus-visible:text-foreground"
          />
        }
      >
        <InfoIcon aria-hidden="true" className="size-3.5" />
      </TooltipTrigger>
      <TooltipContent side="top" className="max-w-64">
        {children}
      </TooltipContent>
    </Tooltip>
  );
}
