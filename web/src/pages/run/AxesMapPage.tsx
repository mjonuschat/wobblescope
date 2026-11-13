import { useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { type ReactNode, useEffect, useRef, useState } from "react";

import { Panel } from "@/components/Panel";
import { Steps } from "@/components/Steps";
import { Button } from "@/components/ui/button";
import { queryKeys, useChips, useStatus } from "@/lib/api/queries";
import type { FinishedTest } from "@/lib/generated/FinishedTest";
import type { TestKind } from "@/lib/generated/TestKind";
import type { TestState } from "@/lib/generated/TestState";

import { AxesMapForm } from "./AxesMapForm";
import { KIND_PAGES, KIND_TITLES } from "./kinds";
import { OUTCOME_ICON, OUTCOME_WORDS, ProgressView, SHUTDOWN_NOTICE, StartingView } from "./ProgressView";
import { useLive } from "./useLive";

const STEPS = [
  { id: "setup", label: "Setup" },
  { id: "measuring", label: "Measuring" },
  { id: "results", label: "Results" },
];

/** What the user is told when the run could not put Klipper's state back. */
export const RESTART_INSTRUCTION =
  "The calibration may have left changed acceleration limits or input shaping. Send RESTART from the Klipper console. The printer needs homing afterwards, and any positioning mode or offsets you rely on must be set again.";

function AxesMapResultView({ last, onNewRun }: { last: FinishedTest; onNewRun: () => void }) {
  const { Icon, className } = OUTCOME_ICON[last.outcome];
  return (
    <Panel
      title="Result"
      label="Calibration result"
      action={
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={onNewRun}
            disabled={last.motion_uncertain}
            title={last.motion_uncertain ? "Stop the printer first: it may still be moving." : undefined}
          >
            New calibration
          </Button>
          {last.session_id && (
            <Button render={<Link to="/sessions/$id" params={{ id: last.session_id }} />}>Open the session</Button>
          )}
        </div>
      }
    >
      <div className="flex items-start gap-3">
        <Icon aria-hidden="true" className={`mt-0.5 size-5 shrink-0 ${className}`} />
        <div className="space-y-1">
          <h2 className="font-medium">{OUTCOME_WORDS[last.outcome]}</h2>
          {last.reason && <p className="text-sm">{last.reason}</p>}
          {last.reason === "emergency stop" && (
            <p role="status" className="text-sm text-muted-foreground">
              {SHUTDOWN_NOTICE}
            </p>
          )}
          {!last.session_id && <p className="text-sm text-muted-foreground">No session was saved.</p>}
        </div>
      </div>
      {last.needs_restart && (
        <p role="alert" className="text-sm text-amber-400">
          {RESTART_INSTRUCTION}
        </p>
      )}
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
}

export function AxesMapPage() {
  const status = useStatus();
  const chips = useChips();
  const live = useLive();
  // The run the form just started, until the live socket names it.
  const [startedId, setStartedId] = useState<string | null>(null);
  // The finished run whose result the user has dismissed.
  const [dismissed, setDismissed] = useState<string | null>(null);
  // The running test as last seen, so its progress can stay on screen when it ends.
  const [lastSeen, setLastSeen] = useState<TestState | null>(null);
  const [advanced, setAdvanced] = useState<string | null>(null);
  // Only a calibration is shown here; a test of another kind is shown on its own page.
  const ownTest = live.test?.kind === "axes_map" ? live.test : null;
  const otherKind: TestKind | null = live.test !== null && !ownTest ? live.test.kind : null;
  useEffect(() => {
    if (ownTest) setLastSeen(ownTest);
  }, [ownTest]);

  const klippyReady = status.data?.moonraker.state === "connected" && status.data.klippy.state === "ready";
  const notReady = klippyReady ? null : "Klipper is not ready.";

  // A JSON error is never retried, so a page opened while Moonraker or Klipper was still
  // starting would keep its empty chip list. Read it again when the printer becomes ready.
  const queryClient = useQueryClient();
  const wasReady = useRef<boolean | null>(null);
  useEffect(() => {
    if (status.data === undefined) return;
    if (klippyReady && wasReady.current === false) {
      void queryClient.invalidateQueries({ queryKey: queryKeys.chips });
    }
    wasReady.current = klippyReady;
  }, [klippyReady, status.data, queryClient]);

  const finished = live.last?.kind === "axes_map" ? live.last : null;
  function newRun() {
    if (finished) setDismissed(finished.id);
    setStartedId(null);
  }

  const starting = startedId !== null && live.last?.id !== startedId;
  const holding =
    !ownTest && !starting && lastSeen !== null && finished?.id === lastSeen.id && advanced !== finished.id;
  const showResult = !ownTest && !starting && !holding && finished !== null && finished.id !== dismissed;
  const step: "setup" | "measuring" | "results" =
    ownTest || starting || holding ? "measuring" : showResult ? "results" : "setup";

  let body: ReactNode;
  if (!live.known && startedId === null) {
    // Until the first snapshot the page cannot tell whether a run is going, so it must not offer
    // the form: starting a second one would be refused, and the running one would stay hidden.
    body = live.connected ? (
      <p className="text-sm text-muted-foreground">Reading the live state…</p>
    ) : (
      <div className="space-y-3 text-sm">
        <p className="text-muted-foreground">Connecting to the live view…</p>
        <p>
          If this stays, the page cannot reach the daemon's live socket, so it cannot tell whether a test is running.
          The printer is not affected, and the stop button above still works.
        </p>
        <Button variant="outline" onClick={live.retry}>
          Try again
        </Button>
      </div>
    );
  } else if (ownTest) {
    body = <ProgressView test={ownTest} connected={live.connected} ended={null} onNext={() => {}} />;
  } else if (holding && lastSeen && finished) {
    body = (
      <ProgressView
        test={lastSeen}
        connected={live.connected}
        ended={finished}
        onNext={() => setAdvanced(finished.id)}
      />
    );
  } else if (starting && startedId) {
    body = <StartingView connected={live.connected} />;
  } else if (showResult && finished) {
    body = <AxesMapResultView last={finished} onNewRun={newRun} />;
  } else {
    body = (
      <>
        {otherKind && (
          <p className="text-sm">
            A {KIND_TITLES[otherKind]} test is running.{" "}
            <Link to={KIND_PAGES[otherKind]} className="underline">
              Go to the {KIND_TITLES[otherKind]} page
            </Link>
          </p>
        )}
        {chips.isPending && <p className="text-sm text-muted-foreground">Loading chips…</p>}
        {chips.isError && (
          <p role="alert" className="text-sm text-destructive">
            {chips.error.message}
          </p>
        )}
        {chips.data && <AxesMapForm chipList={chips.data} blocked={notReady} onStarted={setStartedId} />}
      </>
    );
  }

  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <h1 className="text-xl font-semibold">Axes map calibration</h1>
      <Steps
        steps={STEPS}
        current={step}
        selectable={step === "results" && !finished?.motion_uncertain ? ["setup"] : []}
        onSelect={() => newRun()}
      />
      {body}
    </div>
  );
}
