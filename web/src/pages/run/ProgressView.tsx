import { Link } from "@tanstack/react-router";

import { ArrowRightIcon, LoaderCircleIcon } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import type { FinishedOutcome } from "@/lib/generated/FinishedOutcome";
import type { FinishedTest } from "@/lib/generated/FinishedTest";
import type { Phase } from "@/lib/generated/Phase";
import type { TestState } from "@/lib/generated/TestState";

import { LiveChart } from "./LiveChart";
import { useWarningToasts } from "./useWarningToasts";
import { formatDuration, formatPoint } from "./planning";

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

function PanelCard({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{title}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">{children}</CardContent>
    </Card>
  );
}

/** How long the page waits after a test ends before it moves on to the results, in seconds. */
export const AUTO_CONTINUE_SECONDS = 5;

interface ProgressProps {
  test: TestState;
  /** Whether the live socket is open. */
  connected: boolean;
  /** How the test ended, once it has. The page then counts down to the results. */
  ended: FinishedTest | null;
  /** Moves on to the results. */
  onNext: () => void;
}

/** Counts down from [`AUTO_CONTINUE_SECONDS`] while `active`, and calls `onDone` at zero. */
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
  const where = test.axis && test.position ? ` (${test.axis} at ${formatPoint(test.position)})` : "";
  const percent =
    test.expected_segment_seconds > 0 ? Math.min(100, (test.elapsed_seconds / test.expected_segment_seconds) * 100) : 0;
  return (
    <div className="space-y-4">
      {!connected && (
        <p role="status" className="text-sm text-amber-400">
          Connection lost, this may be out of date.
        </p>
      )}
      <PanelCard title="Progress">
        <div className="space-y-1">
          <p className="font-medium">{ended ? OUTCOME_WORDS[ended.outcome] : PHASE_WORDS[test.phase]}</p>
          <p className="text-sm">
            Segment {test.segment} of {test.segments}
            {where}
          </p>
        </div>
        <Progress value={percent} aria-label="Segment progress" />
        <p className="text-sm text-muted-foreground">
          {formatDuration(test.elapsed_seconds)} of about {formatDuration(test.expected_segment_seconds)}
        </p>
      </PanelCard>
      <PanelCard title="Signal">
        <LiveChart test={test} />
      </PanelCard>
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

/** Shown from the 202 until the live socket names the new test. */
export function StartingView({ connected }: StartingProps) {
  return (
    <div className="space-y-4">
      <PanelCard title="Progress">
        <p role="status" className="flex items-center gap-2">
          <LoaderCircleIcon aria-hidden="true" className="size-4 animate-spin" />
          Starting…
        </p>
        {!connected && (
          <p className="text-sm text-amber-400">
            The live view is not connected. The test is running, but its progress cannot be shown.
          </p>
        )}
      </PanelCard>
    </div>
  );
}

interface ResultProps {
  last: FinishedTest;
  onNewTest: () => void;
}

/** How a test ended, and a way to the session. It stays until the user asks for a new test. */
export function ResultView({ last, onNewTest }: ResultProps) {
  return (
    <div className="mx-auto max-w-2xl space-y-4">
      <h1 className="text-xl font-semibold">{OUTCOME_WORDS[last.outcome]}</h1>
      {last.reason && <p className="text-sm">{last.reason}</p>}
      {last.reason === "emergency stop" && (
        <p role="status" className="text-sm">
          {SHUTDOWN_NOTICE}
        </p>
      )}
      {last.session_id ? (
        <p className="text-sm">
          <Link to="/sessions/$id" params={{ id: last.session_id }} className="underline">
            Open the session
          </Link>
        </p>
      ) : (
        <p className="text-sm text-muted-foreground">No session was saved.</p>
      )}
      {last.motion_uncertain && (
        <div className="space-y-1">
          <p role="alert" className="text-sm text-amber-400">
            The printer may still be moving.
          </p>
          <p className="text-sm text-muted-foreground">Use the stop button at the top right to stop it.</p>
        </div>
      )}
      <Button
        type="button"
        variant="outline"
        onClick={onNewTest}
        disabled={last.motion_uncertain}
        title={last.motion_uncertain ? "Stop the printer first: it may still be moving." : undefined}
      >
        New test
      </Button>
    </div>
  );
}
