import type { StatusResponse } from "@/lib/generated/StatusResponse";
import { homedTone } from "@/components/PrinterControls";
import { useIsMutating } from "@tanstack/react-query";

import { homeMutationKey, levelMutationKey, prepareMutationKey, usePrinterState, useStatus } from "@/lib/api/queries";
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

/** The daemon's busy sentences, as the one word the header shows. */
const ACTIVITY_WORDS: Record<string, string> = {
  "The printer is homing.": "Homing",
  "The printer is levelling.": "Levelling",
  "The chips are being checked.": "Checking chips",
  "A hold is running.": "Hold running",
  "A test is running.": "Test running",
  "The toolhead is moving.": "Moving",
  "The printer may still be moving.": "Moving",
};

/**
 * What the printer is doing. A move started from this browser counts at once, before the poll sees
 * it. `null` while Klipper is not ready, since the banner says so.
 */
export function activityConnection(
  klippyReady: boolean,
  busy: string | null | undefined,
  local: "Homing" | "Levelling" | "Preparing" | null,
): Connection | null {
  if (!klippyReady) return null;
  const word = local ?? (busy ? (ACTIVITY_WORDS[busy] ?? "Busy") : null);
  if (word === null) return { label: "Idle", tone: "off", details: ["Nothing is running"] };
  return { label: word, tone: "warn", details: [busy ?? word] };
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
  const homing = useIsMutating({ mutationKey: homeMutationKey }) > 0;
  const levelling = useIsMutating({ mutationKey: levelMutationKey }) > 0;
  const preparing = useIsMutating({ mutationKey: prepareMutationKey }) > 0;
  const local = preparing ? "Preparing" : levelling ? "Levelling" : homing ? "Homing" : null;
  const activity = activityConnection(klippyReady, printer.data?.busy, local);
  const items = [
    ...connections(status.data),
    homedConnection(klippyReady, printer.data?.homed_axes),
    ...(activity ? [activity] : []),
  ];
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
