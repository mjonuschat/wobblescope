import { useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useEffect, useRef, useState } from "react";

import { Steps } from "@/components/Steps";
import { Button } from "@/components/ui/button";
import { queryKeys, useChips, useSettings, useStatus } from "@/lib/api/queries";
import type { TestState } from "@/lib/generated/TestState";

import { VibrationsForm } from "./VibrationsForm";
import { offersVibrations } from "./kinds";
import { ProgressView, ResultView, StartingView } from "./ProgressView";
import { useLive } from "./useLive";
import { useStartedRun } from "./useStartedRun";
import { PageHeader } from "@/ui-kit/PageHeader";

const STEPS = [
  { id: "setup", label: "Setup" },
  { id: "measuring", label: "Measuring" },
  { id: "results", label: "Results" },
];

export function VibrationsPage() {
  const status = useStatus();
  const settings = useSettings();
  const chips = useChips();
  const live = useLive();
  // The run the form just started, until the live socket names it.
  const [startedId, setStartedId] = useStartedRun(live);
  // The finished run whose result the user has dismissed.
  const [dismissed, setDismissed] = useState<string | null>(null);
  // The running test as last seen, so its progress can stay on screen when it ends.
  const [lastSeen, setLastSeen] = useState<TestState | null>(null);
  const [advanced, setAdvanced] = useState<string | null>(null);
  // Only a vibrations profile is shown here. A test of another kind is shown on its own page.
  const ownTest = live.test?.kind === "vibrations" ? live.test : null;
  useEffect(() => {
    if (ownTest) setLastSeen(ownTest);
  }, [ownTest]);

  const klippyReady = status.data?.moonraker.state === "connected" && status.data.klippy.state === "ready";
  const notReady = klippyReady ? null : "Klipper is not ready.";

  // A JSON error is never retried, so a page opened while Moonraker or Klipper was still
  // starting would keep its error or its empty chip list. Read both again when the printer becomes ready.
  const queryClient = useQueryClient();
  const wasReady = useRef<boolean | null>(null);
  useEffect(() => {
    if (status.data === undefined) return;
    if (klippyReady && wasReady.current === false) {
      void queryClient.invalidateQueries({ queryKey: queryKeys.settings });
      void queryClient.invalidateQueries({ queryKey: queryKeys.chips });
    }
    wasReady.current = klippyReady;
  }, [klippyReady, status.data, queryClient]);

  const finished = live.last?.kind === "vibrations" ? live.last : null;
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
    body = <ResultView last={finished} onNewTest={newRun} />;
  } else {
    body = (
      <>
        {chips.isPending && <p className="text-sm text-muted-foreground">Loading chips…</p>}
        {chips.isError && (
          <p role="alert" className="text-sm text-destructive">
            {chips.error.message}
          </p>
        )}
        {settings.isPending && <p className="text-sm text-muted-foreground">Reading settings…</p>}
        {settings.isError && (
          <p role="alert" className="text-sm text-destructive">
            {settings.error.message}
          </p>
        )}
        {settings.data && !offersVibrations(settings.data.machine.kinematics) && (
          <p className="text-sm">A vibrations profile needs a Cartesian, CoreXY or CoreXZ printer.</p>
        )}
        {settings.data && chips.data && offersVibrations(settings.data.machine.kinematics) && (
          <VibrationsForm chipList={chips.data} blocked={notReady} onStarted={setStartedId} />
        )}
      </>
    );
  }

  return (
    <div className="mx-auto max-w-5xl space-y-6">
      <PageHeader title="Vibrations profile" />
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
