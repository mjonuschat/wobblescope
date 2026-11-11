import { InfoIcon } from "lucide-react";
import type { ReactNode } from "react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

interface FieldProps {
  label: string;
  /** An explanation shown in a tooltip on an info icon after the label. */
  hint?: string;
  className?: string;
  children: ReactNode;
}

/** A small uppercase label over a monospace value. */
export function Field({ label, hint, className, children }: FieldProps) {
  return (
    <div className={className}>
      <dt className="text-xs uppercase tracking-wide text-muted-foreground">
        {hint ? (
          <Tooltip>
            <TooltipTrigger
              render={
                <button
                  type="button"
                  aria-label={`About ${label.toLowerCase()}`}
                  className="-m-1 flex cursor-help items-center gap-1 rounded p-1 uppercase tracking-wide hover:text-foreground focus-visible:text-foreground"
                />
              }
            >
              {label}
              <InfoIcon aria-hidden="true" className="size-3.5" />
            </TooltipTrigger>
            <TooltipContent side="top" className="max-w-64 normal-case tracking-normal">
              {hint}
            </TooltipContent>
          </Tooltip>
        ) : (
          label
        )}
      </dt>
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
