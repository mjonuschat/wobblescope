import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { ChipStreamState } from "@/stream/useStream";

interface Props {
  chip: string;
  state: ChipStreamState | undefined;
  /** The latest spectrum worker failure for this chip. */
  error: string | null;
}

const DOTS = {
  ok: "bg-green-500",
  warning: "bg-amber-500",
  error: "bg-destructive",
} as const;

function count(value: bigint | null | undefined): string {
  return value === null || value === undefined ? "not reported" : String(value);
}

/**
 * One chip's stream health as a dot and its measured rate. The counts and any warning are in a
 * tooltip. Amber is a warning and red is a closed or refused chip or a spectrum failure.
 */
export function HealthBadge({ chip, state, error }: Props) {
  if (!state) return <p className="text-sm text-muted-foreground">Waiting for {chip}…</p>;
  const { health } = state;
  const warnings = health?.warnings ?? [];
  const tone =
    state.closed !== null || state.refused !== null || error !== null
      ? "error"
      : warnings.length > 0
        ? "warning"
        : "ok";
  const measured = health?.measured_rate_hz ?? state.inputRateHz;
  const configured = health ? health.configured_rate_hz : state.configuredRateHz;
  const rate = `${Math.round(measured)} / ${configured === null ? "not configured" : `${configured} Hz`}`;
  const headline =
    state.closed !== null ? "Closed" : state.refused !== null ? "Refused" : error !== null ? "Error" : rate;
  const lines = [
    `Rate: ${Math.round(measured)} Hz of ${configured === null ? "not configured" : `${configured} Hz`}`,
    `Overflows: ${count(health?.overflows)} · Errors: ${count(health?.errors)} · Dropped frames: ${String(health?.dropped_frames ?? 0)}`,
    ...(state.closed !== null ? [`Closed: ${state.closed}`] : []),
    ...(state.refused !== null ? [`Refused: ${state.refused}`] : []),
    ...(error !== null ? [error] : []),
    ...warnings,
  ];
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <span
            data-testid={`health-${chip}`}
            data-tone={tone}
            aria-label={lines.join(". ")}
            tabIndex={0}
            className="inline-flex items-center gap-2 text-sm"
          />
        }
      >
        <span aria-hidden="true" className={`size-2 shrink-0 rounded-full ${DOTS[tone]}`} />
        <span className="font-mono">{headline}</span>
      </TooltipTrigger>
      <TooltipContent className="flex-col items-start">
        {lines.map((line) => (
          <span key={line}>{line}</span>
        ))}
      </TooltipContent>
    </Tooltip>
  );
}
