import { useQueryClient } from "@tanstack/react-query";
import type { TestState } from "@/lib/generated/TestState";
import { type ReactNode, useEffect, useRef, useState } from "react";

import { Steps } from "@/components/Steps";
import { queryKeys, useChips, useSettings, useStatus } from "@/lib/api/queries";

import { ProgressView, ResultView, StartingView } from "./ProgressView";
import { TestForm } from "./TestForm";
import { useLive } from "./useLive";

const STEPS = [
  { id: "setup", label: "Setup" },
  { id: "measuring", label: "Measuring" },
  { id: "results", label: "Results" },
];

/** The Input Shaper test screen. */
export function RunPage() {
  const status = useStatus();
  const settings = useSettings();
  const chips = useChips();
  const live = useLive();
  const [chosen, setChosen] = useState<string[] | null>(null);
  // The test the form just started, until the live socket names it.
  const [startedId, setStartedId] = useState<string | null>(null);
  // The finished test whose result the user has dismissed.
  const [dismissed, setDismissed] = useState<string | null>(null);
  // The running test as last seen, so its progress can stay on screen when it ends.
  const [lastSeen, setLastSeen] = useState<TestState | null>(null);
  // The finished test the user has moved on from, by Next or by the countdown.
  const [advanced, setAdvanced] = useState<string | null>(null);
  useEffect(() => {
    if (live.test) setLastSeen(live.test);
  }, [live.test]);

  const klippyReady = status.data?.moonraker.state === "connected" && status.data.klippy.state === "ready";
  const notReady = klippyReady ? null : "Klipper is not ready.";

  // A JSON error is never retried, so a page opened while Moonraker or Klipper was still
  // starting would keep its error or its empty chip list. Read both again when the printer
  // becomes ready.
  const queryClient = useQueryClient();
  function newTest() {
    if (live.last) setDismissed(live.last.id);
    setStartedId(null);
  }
  const wasReady = useRef<boolean | null>(null);
  useEffect(() => {
    if (status.data === undefined) return;
    if (klippyReady && wasReady.current === false) {
      void queryClient.invalidateQueries({ queryKey: queryKeys.settings });
      void queryClient.invalidateQueries({ queryKey: queryKeys.chips });
    }
    wasReady.current = klippyReady;
  }, [klippyReady, status.data, queryClient]);

  const chipList = chips.data;
  const selected =
    chosen ??
    (chipList ? [...new Set([...chipList.defaults.x, ...chipList.defaults.y])] : []).filter(
      (section) => chipList?.chips.find((c) => c.section === section)?.availability.state !== "unavailable",
    );

  // A chip that turned unavailable cannot record, so it drops out of the selection.
  const usable = selected.filter(
    (s) => chipList?.chips.find((c) => c.section === s)?.availability.state !== "unavailable",
  );

  const finished = live.last;
  const starting = startedId !== null && finished?.id !== startedId;
  const holding =
    !live.test && !starting && lastSeen !== null && finished?.id === lastSeen.id && advanced !== finished.id;
  const showResult = !live.test && !starting && !holding && finished !== null && finished.id !== dismissed;
  const step: "setup" | "measuring" | "results" =
    live.test || starting || holding ? "measuring" : showResult ? "results" : "setup";

  let body: ReactNode;
  if (live.test) {
    body = <ProgressView test={live.test} connected={live.connected} ended={null} onNext={() => {}} />;
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
    body = <ResultView last={finished} onNewTest={newTest} />;
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
        {settings.data && chipList && (
          <TestForm
            settings={settings.data}
            chipList={chipList}
            chips={usable}
            onChipsChange={setChosen}
            blocked={notReady}
            onStarted={setStartedId}
          />
        )}
      </>
    );
  }

  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <h1 className="text-xl font-semibold">Input Shaper</h1>
      <Steps
        steps={STEPS}
        current={step}
        selectable={step === "results" && !finished?.motion_uncertain ? ["setup"] : []}
        onSelect={() => newTest()}
      />
      {body}
    </div>
  );
}
