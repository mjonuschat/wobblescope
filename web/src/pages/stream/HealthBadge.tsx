import type { ChipStreamState } from "@/stream/useStream";

interface Props {
  chip: string;
  state: ChipStreamState | undefined;
  /** The latest spectrum worker failure for this chip. */
  error: string | null;
}

const TONES = {
  ok: "border-border text-muted-foreground",
  warning: "border-amber-500 text-amber-400",
  error: "border-destructive text-destructive",
} as const;

function count(value: bigint | null | undefined): string {
  return value === null || value === undefined ? "not reported" : String(value);
}

/** One chip's stream health. Amber is a warning and red is a closed or refused chip. */
export function HealthBadge({ chip, state, error }: Props) {
  if (!state) return <p className="text-xs text-muted-foreground">Waiting for {chip}…</p>;
  const { health } = state;
  const failed = state.closed !== null || state.refused !== null;
  const warnings = health?.warnings ?? [];
  const tone = failed || error !== null ? "error" : warnings.length > 0 ? "warning" : "ok";
  const measured = health?.measured_rate_hz ?? state.inputRateHz;
  const configured = health ? health.configured_rate_hz : state.configuredRateHz;
  return (
    <div
      data-testid={`health-${chip}`}
      data-tone={tone}
      className={`space-y-1 rounded-md border px-2 py-1 text-xs ${TONES[tone]}`}
    >
      <p>
        Rate: {Math.round(measured)} Hz of {configured === null ? "not configured" : `${configured} Hz`}
      </p>
      <p>
        Overflows: {count(health?.overflows)} · Errors: {count(health?.errors)} · Dropped frames:{" "}
        {String(health?.dropped_frames ?? 0)}
      </p>
      {state.closed !== null && <p>Closed: {state.closed}</p>}
      {state.refused !== null && <p>Refused: {state.refused}</p>}
      {error !== null && <p className="text-destructive">{error}</p>}
      {warnings.map((warning) => (
        <p key={warning}>{warning}</p>
      ))}
    </div>
  );
}
