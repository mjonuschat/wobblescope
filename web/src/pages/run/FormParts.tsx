import { useIsMutating } from "@tanstack/react-query";
import { ListChecksIcon, RotateCcwIcon } from "lucide-react";
import type { ReactNode } from "react";

import { HomeSolidIcon } from "@/components/HomeSolidIcon";
import { homedTone } from "@/components/PrinterControls";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  homeMutationKey,
  levelMutationKey,
  prepareMutationKey,
  useHomePrinter,
  usePreparePrinter,
  usePrinterState,
  useStatus,
} from "@/lib/api/queries";
import { ApiClientError } from "@/lib/api/client";
import { reportError } from "@/lib/notify";
import { CardLabel } from "@/ui-kit/CardLabel";

/** The inline error under a field; `name` is what the field's `aria-describedby` points at. */
export function FieldError({ name, text }: { name: string; text: string | undefined }) {
  if (!text) return null;
  return (
    <p id={`error-${name}`} role="alert" className="text-sm text-destructive">
      {text}
    </p>
  );
}

export function FormCard({ title, action, children }: { title: string; action?: ReactNode; children: ReactNode }) {
  return (
    <Card>
      <CardHeader className="flex items-center justify-between gap-4">
        <CardLabel>{title}</CardLabel>
        {action}
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}

export function ChoiceRow({ label, error, children }: { label: string; error?: ReactNode; children: ReactNode }) {
  return (
    <div className="space-y-2">
      <p className="text-sm font-medium">{label}</p>
      {children}
      {error}
    </div>
  );
}

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
  const levelling = useIsMutating({ mutationKey: levelMutationKey }) > 0;
  const preparing = useIsMutating({ mutationKey: prepareMutationKey }) > 0;
  if (blocked) return blocked;
  if (printer.isPending) return "Checking whether the printer is homed…";
  if (homing) return "The printer is homing.";
  if (levelling) return "The printer is levelling.";
  if (preparing) return "The printer is preparing.";
  if (printer.data?.busy) return printer.data.busy;
  if (printer.data && homedTone(printer.data.homed_axes) !== "full") {
    const axes = printer.data.homed_axes ?? "";
    const notHomed = ["X", "Y", "Z"].filter((a) => !axes.toLowerCase().includes(a.toLowerCase()));
    return `Home the printer first. Not homed: ${notHomed.join(", ")}.`;
  }
  return null;
}

/**
 * Gets the printer ready from a run form: PREP (home, level, home Z) where the printer can level,
 * else HOME ALL. It shows only while the printer is not fully homed.
 */
export function HomeButton() {
  const status = useStatus();
  const ready = status.data?.moonraker.state === "connected" && status.data.klippy.state === "ready";
  const printer = usePrinterState(ready);
  const home = useHomePrinter();
  const prepare = usePreparePrinter();
  const levelling = useIsMutating({ mutationKey: levelMutationKey }) > 0;
  if (!ready || !printer.data || homedTone(printer.data.homed_axes) === "full") return null;
  const canLevel = printer.data.level_kind != null;
  const disabled = home.isPending || prepare.isPending || levelling || printer.data.busy !== null;
  return (
    <>
      {canLevel ? (
        <Button
          type="button"
          variant="outline"
          size="lg"
          aria-label="Prepare the printer"
          disabled={disabled}
          title={`Runs G28, ${printer.data.level_command ?? "the levelling"}, then G28 Z`}
          onClick={() => prepare.mutate()}
        >
          <ListChecksIcon data-icon="inline-start" />
          PREP
        </Button>
      ) : (
        <Button
          type="button"
          variant="outline"
          size="lg"
          aria-label="Home all"
          disabled={disabled}
          onClick={() => home.mutate("all")}
        >
          <HomeSolidIcon data-icon="inline-start" />
          HOME ALL
        </Button>
      )}
    </>
  );
}

/** Reports a failed start that has no list of reasons to show beside the Run button. */
export function reportStartFailure(error: Error) {
  const body = error instanceof ApiClientError ? error.body : null;
  if (body?.error !== "preflight_failed") reportError(error.message);
}
