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

import { Panel } from "@/components/Panel";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import type { FinishedOutcome } from "@/lib/generated/FinishedOutcome";
import type { FinishedTest } from "@/lib/generated/FinishedTest";
import type { Phase } from "@/lib/generated/Phase";
import type { TestState } from "@/lib/generated/TestState";

import { SessionResults } from "@/pages/session/SessionResults";

import { LiveChart } from "./LiveChart";
import { useWarningToasts } from "./useWarningToasts";
import { formatDuration, formatPosition, segmentLabel } from "./planning";

const PHASE_WORDS: Record<Phase, string> = {
  positioning: "Moving to the test point",
  measuring: "Measuring",
  draining: "Collecting the last samples",
  waiting_for_sweep: "Failure recorded, waiting for the sweep to end",
  sealing: "Saving the session",
};

const OUTCOME_WORDS: Record<FinishedOutcome, string> = {
  complete: "The test finished.",
  incomplete: "The test finished with missing data.",
  failed: "The test failed.",
  no_session: "The test failed before it recorded anything.",
};

/** What the user is told after an emergency stop. */
export const SHUTDOWN_NOTICE =
  "Emergency stop sent. Klipper is in shutdown and needs a FIRMWARE_RESTART before it moves again.";

/** What to try when the printer's Klipper refuses a diagonal axis. */
export const BELT_MINIMUMS = "Belt tests need Klipper v0.12.0 or newer, or Kalico v2026.01.00 or newer.";

export const AUTO_CONTINUE_SECONDS = 5;

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

/**
 * A running test: where it is, how long it has run and each chip's signal. When the test has
 * ended it stays for a few seconds, so the last of the graph can be seen, and then goes on.
 */
export function ProgressView({ test, connected, ended, onNext }: ProgressProps) {
  useWarningToasts(test.id, test.chips);
  const left = useCountdown(ended !== null, onNext);
  const segment = `${test.segment} of ${test.segments}${test.axis ? `: ${segmentLabel(test.axis)}` : ""}`;
  const percent =
    test.expected_segment_seconds > 0 ? Math.min(100, (test.elapsed_seconds / test.expected_segment_seconds) * 100) : 0;
  return (
    <div className="space-y-4">
      {!connected && (
        <p role="status" className="text-sm text-amber-400">
          Connection lost, this may be out of date.
        </p>
      )}
      <Panel title="Progress">
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
          <dt className="text-muted-foreground">Status</dt>
          <dd>{ended ? OUTCOME_WORDS[ended.outcome] : PHASE_WORDS[test.phase]}</dd>
          <dt className="text-muted-foreground">Segment</dt>
          <dd>{segment}</dd>
          {test.position && (
            <>
              <dt className="text-muted-foreground">Position</dt>
              <dd>{formatPosition(test.position)}</dd>
            </>
          )}
        </dl>
        <Progress value={percent} aria-label="Segment progress" />
        <p className="text-sm text-muted-foreground">
          {formatDuration(test.elapsed_seconds)} of about {formatDuration(test.expected_segment_seconds)}
        </p>
      </Panel>
      <Panel title="Signal">
        <LiveChart test={test} />
      </Panel>
      <Separator />
      <div className="flex items-center justify-between gap-4">
        <p role="status" className="text-sm text-muted-foreground">
          {left !== null ? `Continuing to the results in ${left} s` : ""}
        </p>
        <Button
          type="button"
          size="lg"
          className="ml-auto bg-neutral-300 px-5 text-neutral-900 hover:bg-neutral-200"
          disabled={ended === null}
          onClick={onNext}
        >
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
          <p className="text-sm text-amber-400">
            The live view is not connected. The test is running, but its progress cannot be shown.
          </p>
        )}
      </Panel>
    </div>
  );
}

interface ResultProps {
  last: FinishedTest;
  onNewTest: () => void;
}

const OUTCOME_ICON: Record<FinishedOutcome, { Icon: LucideIcon; className: string }> = {
  complete: { Icon: CircleCheckIcon, className: "text-emerald-400" },
  incomplete: { Icon: TriangleAlertIcon, className: "text-amber-400" },
  failed: { Icon: OctagonXIcon, className: "text-red-400" },
  no_session: { Icon: OctagonXIcon, className: "text-red-400" },
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
  const card = (save: ReactNode) => (
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
      {last.motion_uncertain && (
        <div className="space-y-1">
          <p role="alert" className="text-sm text-amber-400">
            The printer may still be moving.
          </p>
          <p className="text-sm text-muted-foreground">Use the stop button at the top right to stop it.</p>
        </div>
      )}
    </Panel>
  );
  return analysed && last.session_id ? <SessionResults id={last.session_id} header={card} /> : card(null);
}
