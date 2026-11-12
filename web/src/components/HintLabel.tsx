import { InfoIcon } from "lucide-react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/** A form label that explains itself in a tooltip when it is focused or hovered. */
export function HintLabel({ label, hint }: { label: string; hint: string }) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            aria-label={`About ${label.toLowerCase()}`}
            className="-mx-1 flex cursor-help items-center gap-1 rounded px-1 py-0.5 text-sm leading-none font-medium"
          />
        }
      >
        {label}
        <InfoIcon aria-hidden="true" className="size-3.5 text-muted-foreground" />
      </TooltipTrigger>
      <TooltipContent side="top" className="max-w-64">
        {hint}
      </TooltipContent>
    </Tooltip>
  );
}
