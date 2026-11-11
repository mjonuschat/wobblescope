import { HouseIcon, LoaderCircleIcon, OctagonAlertIcon } from "lucide-react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { CONFIRM_WINDOW_MS, useConfirmClick } from "@/lib/confirm";
import { useHomePrinter, usePrinterState, useStatus, useStopPrinter } from "@/lib/api/queries";
import { cn } from "@/lib/utils";

/** How much of the printer is homed: all of X, Y and Z, some of them, or none. */
export function homedTone(axes: string | null | undefined): "full" | "partial" | "none" {
  if (!axes) return "none";
  return ["x", "y", "z"].every((a) => axes.includes(a)) ? "full" : "partial";
}

/** The emergency stop and home buttons for the whole printer, at the right end of the header. */
export function PrinterControls() {
  const status = useStatus();
  const klippyReady = status.data?.moonraker.state === "connected" && status.data.klippy.state === "ready";
  const printer = usePrinterState(klippyReady);
  const stop = useStopPrinter();
  const home = useHomePrinter();
  const confirm = useConfirmClick(() => stop.mutate());

  const homedAxes = printer.data?.homed_axes ?? "";
  const homedClass = {
    full: "text-emerald-400",
    partial: "text-amber-400",
    none: "",
  }[homedTone(homedAxes)];
  const stopping = stop.isPending;
  const stopLabel = stopping ? "Stopping printer" : confirm.armed ? "Press again to stop printer" : "Stop printer";

  return (
    <div className="ml-auto flex items-center gap-3">
      {home.isError && <span className="text-amber-400">{home.error.message}</span>}
      {stop.isError && <span className="text-red-400">{stop.error.message}</span>}
      {stop.isSuccess && <span className="font-mono text-red-400">Stop sent</span>}
      <Tooltip>
        <TooltipTrigger
          render={
            <button
              type="button"
              aria-label={stopLabel}
              aria-pressed={confirm.armed}
              disabled={stopping}
              onClick={confirm.click}
              className={cn(
                "flex h-9 items-center rounded-md border px-2 transition-colors focus-visible:outline-2 disabled:opacity-50",
                confirm.armed
                  ? "border-red-500 bg-red-600 text-white hover:bg-red-500"
                  : "text-red-400 hover:bg-accent",
              )}
            />
          }
        >
          {stopping ? <LoaderCircleIcon className="size-5 animate-spin" /> : <OctagonAlertIcon className="size-5" />}
        </TooltipTrigger>
        <TooltipContent side="bottom" align="end">
          {confirm.armed
            ? `Click again within ${CONFIRM_WINDOW_MS / 1000} seconds to stop.`
            : "Emergency stop. Click twice to confirm."}
        </TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger
          render={
            <button
              type="button"
              aria-label={home.isPending ? "Homing" : "Home"}
              disabled={home.isPending || !klippyReady}
              onClick={() => home.mutate()}
              className={cn(
                "flex h-9 items-center rounded-md border px-2 transition-colors hover:bg-accent focus-visible:outline-2 disabled:cursor-not-allowed disabled:opacity-50",
                homedClass,
              )}
            />
          }
        >
          {home.isPending ? <LoaderCircleIcon className="size-5 animate-spin" /> : <HouseIcon className="size-5" />}
        </TooltipTrigger>
        <TooltipContent side="bottom" align="end">
          Home all axes (homed: {homedAxes === "" ? "none" : homedAxes})
        </TooltipContent>
      </Tooltip>
    </div>
  );
}
