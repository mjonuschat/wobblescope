import type { StatusResponse } from "@/lib/generated/StatusResponse";
import { useStatus } from "@/lib/api/queries";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

export type Tone = "ok" | "warn" | "error" | "off";

interface Connection {
  label: string;
  tone: Tone;
  details: string[];
}

/** One light per link in the chain: Moonraker, Klipper behind it, and the session storage. */
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

export function Dot({ tone }: { tone: Tone }) {
  const color = {
    ok: "bg-emerald-400",
    warn: "bg-amber-400",
    error: "bg-red-400",
    off: "bg-muted-foreground/50",
  }[tone];
  return <span aria-hidden="true" className={cn("inline-block size-2 rounded-full", color)} />;
}

/** The connection lights in the header, each with its details in a tooltip. */
export function StatusDots() {
  const status = useStatus();
  const items = connections(status.data);
  return (
    <ul aria-label="Connections" className="flex items-center gap-x-4">
      {items.map(({ label, tone, details }) => (
        <li key={label}>
          <Tooltip>
            <TooltipTrigger render={<div tabIndex={0} className="flex items-center gap-2" />}>
              <Dot tone={tone} />
              <span>{label}</span>
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
