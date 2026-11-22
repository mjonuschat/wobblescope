import type { StatusResponse } from "@/lib/generated/StatusResponse";
import { homedTone } from "@/components/PrinterControls";
import { usePrinterState, useStatus } from "@/lib/api/queries";
import { cn } from "@/lib/utils";
import { toneDot } from "@/ui-kit/tone";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

export type Tone = "ok" | "warn" | "error" | "off";

interface Connection {
  label: string;
  tone: Tone;
  details: string[];
}

export function connections(status: StatusResponse | undefined): Connection[] {
  if (!status) {
    return ["Moonraker", "Klipper", "Storage"].map((label) => ({ label, tone: "off", details: ["No answer yet"] }));
  }
  const { moonraker, klippy, storage } = status;
  const moonrakerTone: Tone =
    moonraker.state === "connected" ? "ok" : moonraker.state === "connecting" ? "warn" : "error";
  const klippyTone: Tone =
    klippy.state === "ready"
      ? "ok"
      : klippy.state === "startup"
        ? "warn"
        : klippy.state === "error" || klippy.state === "shutdown"
          ? "error"
          : "off";
  return [
    {
      label: "Moonraker",
      tone: moonrakerTone,
      details: [
        `State: ${moonraker.state}`,
        ...(moonraker.version ? [`Version: ${moonraker.version}`] : []),
        ...(moonraker.message ? [moonraker.message] : []),
      ],
    },
    {
      label: "Klipper",
      tone: klippyTone,
      details: [`State: ${klippy.state}`, ...(klippy.message ? [klippy.message] : [])],
    },
    {
      label: "Storage",
      tone: storage.state === "ready" ? "ok" : "warn",
      details: [storage.state === "ready" ? "Sessions can be stored" : "Waiting for Moonraker to name a location"],
    },
  ];
}

/** The Homed item. An unhomed printer is not a fault, so it is never red. */
export function homedConnection(klippyReady: boolean, homedAxes: string | null | undefined): Connection {
  const axes = (homedAxes ?? "").toLowerCase();
  const homed = ["x", "y", "z"].filter((a) => axes.includes(a)).map((a) => a.toUpperCase());
  const tone = klippyReady ? homedTone(axes) : "none";
  return {
    label: "Homed",
    tone: { full: "ok", partial: "warn", none: "off" }[tone] as Tone,
    details: [
      !klippyReady ? "Klipper is not ready" : homed.length === 0 ? "No axes homed" : `Homed axes: ${homed.join(", ")}`,
    ],
  };
}

export function Dot({ tone }: { tone: Tone }) {
  const color = {
    ok: toneDot.success,
    warn: toneDot.warning,
    error: toneDot.danger,
    off: "bg-muted-foreground/50",
  }[tone];
  return <span aria-hidden="true" className={cn("inline-block size-2 rounded-full", color)} />;
}

export function StatusDots() {
  const status = useStatus();
  const klippyReady = status.data?.moonraker.state === "connected" && status.data.klippy.state === "ready";
  const printer = usePrinterState(klippyReady);
  const items = [...connections(status.data), homedConnection(klippyReady, printer.data?.homed_axes)];
  return (
    <ul aria-label="Connections" className="flex items-center gap-x-3 sm:gap-x-4">
      {items.map(({ label, tone, details }) => (
        <li key={label}>
          <Tooltip>
            <TooltipTrigger render={<div tabIndex={0} className="flex items-center gap-2" />}>
              <Dot tone={tone} />
              <span className="max-sm:sr-only">{label}</span>
              <span className="sr-only">{details[0]}</span>
            </TooltipTrigger>
            <TooltipContent side="bottom" align="start">
              {details.map((line) => (
                <p key={line}>{line}</p>
              ))}
            </TooltipContent>
          </Tooltip>
        </li>
      ))}
    </ul>
  );
}
