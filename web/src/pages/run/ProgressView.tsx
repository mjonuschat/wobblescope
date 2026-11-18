import {
  ArrowRightIcon,
  CircleCheckIcon,
  LoaderCircleIcon,
  type LucideIcon,
  OctagonXIcon,
  TriangleAlertIcon,
} from "lucide-react";
import type { ReactNode } from "react";
import { useEffect, useState } from "react";

import { Panel } from "@/ui-kit/Panel";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import { formatMm } from "@/lib/parse";
import type { FinishedOutcome } from "@/lib/generated/FinishedOutcome";
import type { FinishedTest } from "@/lib/generated/FinishedTest";
import type { Phase } from "@/lib/generated/Phase";
import type { TestState } from "@/lib/generated/TestState";

import { CHIP_AXIS_COLORS } from "@/pages/session/AxesMapPlot";
import { SessionResults } from "@/pages/session/SessionResults";

import { LiveChart } from "./LiveChart";
import { useWarningToasts } from "./useWarningToasts";
import { formatDuration, formatPosition, segmentLabel } from "./planning";
import { Alert } from "@/ui-kit/Alert";

const PHASE_WORDS: Record<Phase, string> = {
  positioning: "Moving to the test point",
  measuring: "Measuring",
  draining: "Collecting the last samples",
  waiting_for_sweep: "Failure recorded, waiting for the sweep to end",
  sealing: "Saving the session",
};

export const OUTCOME_WORDS: Record<FinishedOutcome, string> = {
  complete: "The test finished.",
  incomplete: "The test finished with missing data.",
  failed: "The test failed.",
  no_session: "The test failed before it recorded anything.",
};

/** What the user is told after an emergency stop. */
export const SHUTDOWN_NOTICE =
  "Emergency stop sent. Klipper is in shutdown and needs a FIRMWARE_RESTART before it moves again.";

/** The move of each axes-map segment, in the order the daemon runs them. */
const AXES_MAP_MOVES = ["+X", "+Y", "+Z"];

/** What to try when the printer's Klipper refuses a diagonal axis. */
export const BELT_MINIMUMS = "Belt tests need Klipper v0.12.0 or newer, or Kalico v2026.01.00 or newer.";

export const AUTO_CONTINUE_SECONDS = 5;

export const RESTART_INSTRUCTION =
  "The calibration may have left changed acceleration limits or input shaping. Send RESTART from the Klipper console. The printer needs homing afterwards, and any positioning mode or offsets you rely on must be set again.";

interface ProgressProps {
  test: TestState;
  /** Whether the live socket is open. */
  connected: boolean;
  ended: FinishedTest | null;
  onNext: () => void;
}

function useCountdown(active: boolean, onDone: () => void): number | null {
  const [left, setLeft] = useState(AUTO_CONTINUE_SECONDS);
  useEffect(() => {
    if (!active) {
      setLeft(AUTO_CONTINUE_SECONDS);
      return;
    }
    const timer = setInterval(() => setLeft((n) => n - 1), 1000);
    return () => clearInterval(timer);
  }, [active]);
  useEffect(() => {
    if (active && left <= 0) onDone();
  }, [active, left, onDone]);
  return active ? Math.max(left, 0) : null;
}

/** The level at which a signal bar is full, in mm/s². Display only. The bar is on a square-root scale. */
const LEVEL_FULL = 1000;

/**
 * Each chip's vibration on its own axes while a calibration moves. The bar for the axis that
 * follows the current move should rise well above the others.
 */
function SignalLevels({ test }: { test: TestState }) {
  const shown = test.chips.filter((c) => c.level);
  if (shown.length === 0) return <p className="text-sm text-muted-foreground">Waiting for the chips…</p>;
  return (
    <div className="space-y-4">
      {shown.map(({ chip, level }) => (
        <section key={chip} aria-label={`Signal level of ${chip}`} className="space-y-1.5">
          <h3 className="font-mono text-sm">{chip}</h3>
          <ul className="grid grid-cols-[auto_1fr_6rem] items-center gap-x-3 gap-y-1 text-sm">
            {(level ?? []).map((rms, i) => (
              <li key={"xyz"[i]} className="contents">
                <span className="font-medium">{"xyz"[i]}</span>
                <span aria-hidden="true" className="h-1.5 overflow-hidden rounded-full bg-muted">
                  <span
                    className="block h-full rounded-full"
                    style={{
                      width: `${Math.min(1, Math.sqrt(rms / LEVEL_FULL)) * 100}%`,
                      backgroundColor: CHIP_AXIS_COLORS[i],
                    }}
                  />
                </span>
                <span className="text-right tabular-nums text-muted-foreground">
                  {Math.round(rms)} <span className="text-xs">mm/s²</span>
                </span>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

/**
 * A running test: where it is, how long it has run and each chip's signal. When the test has
 * ended it stays for a few seconds, so the last of the graph can be seen, and then goes on.
 */
export function ProgressView({ test, connected, ended, onNext }: ProgressProps) {
  useWarningToasts(test.id, test.chips);
  const left = useCountdown(ended !== null, onNext);
  const axesMap = test.kind === "axes_map";
  // Axes-map and vibrations runs have no frequency sweep to draw, only the chips' latest levels.
  const levels = axesMap || test.kind === "vibrations";
  const move = axesMap ? AXES_MAP_MOVES[test.segment - 1] : undefined;
  const label = move ? `Moving ${move}` : test.axis ? segmentLabel(test.axis) : null;
  // A run that completed has done every segment, whatever the last live snapshot said.
  const segment =
    ended?.outcome === "complete"
      ? `${test.segments} of ${test.segments}`
      : `${test.segment} of ${test.segments}${label ? `: ${label}` : ""}`;
  const percent =
    test.expected_segment_seconds > 0 ? Math.min(100, (test.elapsed_seconds / test.expected_segment_seconds) * 100) : 0;
  return (
    <div className="space-y-4">
      {!connected && (
        <Alert tone="warning" variant="inline">
          Connection lost, this may be out of date.
        </Alert>
      )}
      <Panel title="Progress">
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
          <dt className="text-muted-foreground">Status</dt>
          <dd>{ended ? OUTCOME_WORDS[ended.outcome] : PHASE_WORDS[test.phase]}</dd>
          <dt className="text-muted-foreground">Segment</dt>
          <dd>{segment}</dd>
          {test.position && ended === null && (
            <>
              <dt className="text-muted-foreground">Position</dt>
              <dd className="flex items-baseline gap-4 tabular-nums">
                <span className="sr-only">{formatPosition(test.position)}</span>
                {(["X", "Y", "Z"] as const).map((axis, i) => (
                  <span key={axis} aria-hidden="true">
                    <span className="mr-1 text-muted-foreground">{axis}</span>
                    {formatMm(test.position?.[i] ?? 0)}
                  </span>
                ))}
                <span aria-hidden="true" className="text-muted-foreground">
                  mm
                </span>
              </dd>
            </>
          )}
        </dl>
        {ended === null && (
          <>
            <Progress value={percent} aria-label="Segment progress" />
            <p className="text-sm text-muted-foreground">
              {formatDuration(test.elapsed_seconds)} of about {formatDuration(test.expected_segment_seconds)}
            </p>
          </>
        )}
      </Panel>
      {/* A sweep's graph stays so its last part can be seen. The calibration's levels are only the last
          live reading, which says nothing once the chips have stopped. */}
      {!(levels && ended !== null) && (
        <Panel title="Signal">{levels ? <SignalLevels test={test} /> : <LiveChart test={test} />}</Panel>
      )}
      <Separator />
      <div className="flex items-center justify-between gap-4">
        <p role="status" className="text-sm text-muted-foreground">
          {left !== null ? `Continuing to the results in ${left} s` : ""}
        </p>
        <Button type="button" size="lg" className="ml-auto px-5" disabled={ended === null} onClick={onNext}>
          Next
          <ArrowRightIcon data-icon="inline-end" />
        </Button>
      </div>
    </div>
  );
}

interface StartingProps {
  /** Whether the live socket is open. */
  connected: boolean;
}

export function StartingView({ connected }: StartingProps) {
  return (
    <div className="space-y-4">
      <Panel title="Progress">
        <p role="status" className="flex items-center gap-2">
          <LoaderCircleIcon aria-hidden="true" className="size-4 animate-spin" />
          Starting…
        </p>
        {!connected && (
          <Alert tone="warning" variant="inline">
            The live view is not connected. The test is running, but its progress cannot be shown.
          </Alert>
        )}
      </Panel>
    </div>
  );
}

interface ResultProps {
  last: FinishedTest;
  onNewTest: () => void;
}

export const OUTCOME_ICON: Record<FinishedOutcome, { Icon: LucideIcon; className: string }> = {
  complete: { Icon: CircleCheckIcon, className: "text-success" },
  incomplete: { Icon: TriangleAlertIcon, className: "text-warning" },
  failed: { Icon: OctagonXIcon, className: "text-destructive" },
  no_session: { Icon: OctagonXIcon, className: "text-destructive" },
};

/**
 * How a test ended, with a way to a new test. A test that recorded data also shows its session
 * below. It stays until the user asks for a new test.
 */
export function ResultView({ last, onNewTest }: ResultProps) {
  const { Icon, className } = OUTCOME_ICON[last.outcome];
  const analysed = last.session_id !== null && (last.outcome === "complete" || last.outcome === "incomplete");
  const announced = last.outcome !== "complete";
  const notes = (
    <>
      {last.reason && <p className="text-sm">{last.reason}</p>}
      {last.code === "diagonal_axis_refused" && <p className="text-sm text-muted-foreground">{BELT_MINIMUMS}</p>}
      {last.reason === "emergency stop" && (
        <p role="status" className="text-sm text-muted-foreground">
          {SHUTDOWN_NOTICE}
        </p>
      )}
      {!last.session_id && <p className="text-sm text-muted-foreground">No session was saved.</p>}
    </>
  );
  const hasNotes = Boolean(last.reason) || last.code === "diagonal_axis_refused" || !last.session_id;
  // A completed test is not announced; its notes, if any, stand alone.
  const details = announced ? (
    <div className="flex items-start gap-3">
      <Icon aria-hidden="true" className={`mt-0.5 size-5 shrink-0 ${className}`} />
      <div className="space-y-1">
        <h2 className="font-medium">{OUTCOME_WORDS[last.outcome]}</h2>
        {notes}
      </div>
    </div>
  ) : hasNotes ? (
    <div className="space-y-1">{notes}</div>
  ) : null;
  const card = (save: ReactNode, summary?: ReactNode) => (
    <Panel
      title="Result"
      label="Test result"
      action={
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={onNewTest}
            disabled={last.motion_uncertain}
            title={last.motion_uncertain ? "Stop the printer first: it may still be moving." : undefined}
          >
            New test
          </Button>
          {save}
        </div>
      }
    >
      {details}
      {summary}
      {last.needs_restart && (
        <Alert tone="warning" variant="inline" role="alert">
          {RESTART_INSTRUCTION}
        </Alert>
      )}
      {last.motion_uncertain && (
        <div className="space-y-1">
          <Alert tone="warning" variant="inline" role="alert">
            The printer may still be moving.
          </Alert>
          <p className="text-sm text-muted-foreground">Use the stop button at the top right to stop it.</p>
        </div>
      )}
    </Panel>
  );
  return analysed && last.session_id ? <SessionResults id={last.session_id} header={card} /> : card(null);
}
