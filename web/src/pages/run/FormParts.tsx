import { useIsMutating } from "@tanstack/react-query";
import { RotateCcwIcon } from "lucide-react";
import type { ReactNode } from "react";

import { homedTone } from "@/components/PrinterControls";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { homeMutationKey, usePrinterState } from "@/lib/api/queries";

/** The inline error under a field; `name` is what the field's `aria-describedby` points at. */
export function FieldError({ name, text }: { name: string; text: string | undefined }) {
  if (!text) return null;
  return (
    <p id={`error-${name}`} role="alert" className="text-sm text-destructive">
      {text}
    </p>
  );
}

/** A titled card of a run form, with an optional control in the header. */
export function FormCard({ title, action, children }: { title: string; action?: ReactNode; children: ReactNode }) {
  return (
    <Card>
      <CardHeader className="flex items-center justify-between gap-4">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{title}</CardTitle>
        {action}
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}

/** A labelled group of controls with its error underneath. */
export function ChoiceRow({ label, error, children }: { label: string; error?: ReactNode; children: ReactNode }) {
  return (
    <div className="space-y-2">
      <p className="text-sm font-medium">{label}</p>
      {children}
      {error}
    </div>
  );
}

/** The header button that puts a form's overrides back to the printer's values. */
export function ResetButton({ disabled, onClick }: { disabled: boolean; onClick: () => void }) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            type="button"
            variant="outline"
            size="icon"
            aria-label="Reset"
            disabled={disabled}
            onClick={onClick}
          />
        }
      >
        <RotateCcwIcon />
      </TooltipTrigger>
      <TooltipContent side="top">Back to the printer's values</TooltipContent>
    </Tooltip>
  );
}

/**
 * Why a run cannot start for want of a ready, homed printer, or `null`. `blocked` is the page's own
 * reason and wins. A Home pressed in this browser counts at once, since the poll may lag behind it.
 */
export function usePrinterProblem(blocked: string | null): string | null {
  const printer = usePrinterState(true);
  const homing = useIsMutating({ mutationKey: homeMutationKey }) > 0;
  if (blocked) return blocked;
  if (printer.isPending) return "Checking whether the printer is homed…";
  if (homing) return "The printer is homing.";
  if (printer.data?.busy) return printer.data.busy;
  if (printer.data && homedTone(printer.data.homed_axes) !== "full") {
    const axes = printer.data.homed_axes ?? "";
    const notHomed = ["X", "Y", "Z"].filter((a) => !axes.toLowerCase().includes(a.toLowerCase()));
    return `Home the printer first. Not homed: ${notHomed.join(", ")}.`;
  }
  return null;
}
