import { Link } from "@tanstack/react-router";

import { LoaderCircleIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import { useEmergencyStop } from "@/lib/api/queries";
import type { ChipLive } from "@/lib/generated/ChipLive";
import type { FinishedOutcome } from "@/lib/generated/FinishedOutcome";
import type { FinishedTest } from "@/lib/generated/FinishedTest";
import type { Phase } from "@/lib/generated/Phase";
import type { TestState } from "@/lib/generated/TestState";

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

/** The red button, at the right under a separator. It never asks for confirmation. */
function StopButton({ id }: { id: string }) {
  const stop = useEmergencyStop();
  return (
    <>
      <Separator />
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-2">
          {stop.isSuccess && (
            <p role="status" className="text-sm">
              {SHUTDOWN_NOTICE}
            </p>
          )}
          {stop.isError && (
            <p role="alert" className="text-sm text-destructive">
              {stop.error.message}
            </p>
          )}
        </div>
        <Button type="button" variant="destructive" size="lg" className="ml-auto px-5" onClick={() => stop.mutate(id)}>
          Emergency stop
        </Button>
      </div>
    </>
  );
}

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

/** A bar for a chip's signal on a log scale, so a quiet and a loud chip are both visible. */
function LevelBar({ chip }: { chip: ChipLive }) {
  const peak = chip.level ? Math.max(...chip.level) : 0;
  const percent = Math.min(100, (Math.log10(1 + peak) / 4) * 100);
  return (
    <div className="space-y-1">
      <div className="flex justify-between text-sm">
        <span>{chip.chip}</span>
        <span className="text-muted-foreground">{chip.level ? `${peak.toFixed(0)} mm/s²` : "no signal yet"}</span>
      </div>
      {/* biome-ignore lint/a11y/useSemanticElements: a styled bar; <meter> cannot be styled to match */}
      <div
        role="meter"
        aria-label={`${chip.chip} signal level`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(percent)}
        className="h-2 overflow-hidden rounded-full bg-muted"
      >
        <div className="h-2 rounded-full bg-primary" style={{ width: `${percent}%` }} />
      </div>
      {chip.warnings.length > 0 && (
        <ul className="list-disc pl-5 text-sm text-amber-400">
          {chip.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

interface ProgressProps {
  test: TestState;
  /** Whether the live socket is open. */
  connected: boolean;
}

/** A running test: where it is, how long it has run, each chip's signal and the stop button. */
export function ProgressView({ test, connected }: ProgressProps) {
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
          <p className="font-medium">{PHASE_WORDS[test.phase]}</p>
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
        {test.chips.map((chip) => (
          <LevelBar key={chip.chip} chip={chip} />
        ))}
      </PanelCard>
      <StopButton id={test.id} />
    </div>
  );
}

interface StartingProps {
  /** The id the daemon accepted. */
  id: string;
  /** Whether the live socket is open. */
  connected: boolean;
}

/**
 * Shown from the 202 until the live socket names the new test. The test is already running, so
 * the stop button is here too: a proxy that refuses the websocket must not leave the user with
 * no way to stop it.
 */
export function StartingView({ id, connected }: StartingProps) {
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
      <StopButton id={id} />
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
        <div className="space-y-2">
          <p role="alert" className="text-sm text-amber-400">
            The printer may still be moving.
          </p>
          <StopButton id={last.id} />
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
