import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import { ApiClientError } from "@/lib/api/client";
import { queryKeys, useChips, useCheckChips, useSettings, useStatus } from "@/lib/api/queries";

import { ChipsPanel } from "./ChipsPanel";
import { ProgressView, ResultView, StartingView } from "./ProgressView";
import { TestForm } from "./TestForm";
import { useLive } from "./useLive";

/** The Run test screen: the chips panel and the test form. */
export function RunPage() {
  const status = useStatus();
  const settings = useSettings();
  const chips = useChips();
  const check = useCheckChips();
  const live = useLive();
  const [chosen, setChosen] = useState<string[] | null>(null);
  // The test the form just started, until the live socket names it.
  const [startedId, setStartedId] = useState<string | null>(null);
  // The finished test whose result the user has dismissed.
  const [dismissed, setDismissed] = useState<string | null>(null);

  const klippyReady = status.data?.moonraker.state === "connected" && status.data.klippy.state === "ready";
  const notReady = klippyReady ? null : "Klipper is not ready.";

  // A JSON error is never retried, so a page opened while Moonraker or Klipper was still
  // starting would keep its error or its empty chip list. Read both again when the printer
  // becomes ready.
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

  const chipList = chips.data;
  const selected =
    chosen ??
    (chipList ? [...new Set([...chipList.defaults.x, ...chipList.defaults.y])] : []).filter(
      (section) => chipList?.chips.find((c) => c.section === section)?.availability.state !== "unavailable",
    );

  function toggle(section: string) {
    setChosen((old) => {
      const current = old ?? selected;
      return current.includes(section) ? current.filter((s) => s !== section) : [...current, section];
    });
  }

  // A chip that turned unavailable cannot record, so it drops out of the selection.
  const usable = selected.filter(
    (s) => chipList?.chips.find((c) => c.section === s)?.availability.state !== "unavailable",
  );

  if (live.test) return <ProgressView test={live.test} connected={live.connected} />;
  const finished = live.last;
  if (startedId && finished?.id !== startedId) {
    return <StartingView id={startedId} connected={live.connected} />;
  }
  if (finished && finished.id !== dismissed) {
    return (
      <ResultView
        last={finished}
        onNewTest={() => {
          setDismissed(finished.id);
          setStartedId(null);
        }}
      />
    );
  }

  return (
    <div className="mx-auto max-w-4xl space-y-8">
      <h1 className="text-xl font-semibold">Run test</h1>
      {chips.isPending && <p className="text-sm text-muted-foreground">Loading chips…</p>}
      {chips.isError && (
        <p role="alert" className="text-sm text-destructive">
          {chips.error.message}
        </p>
      )}
      {chipList && (
        <ChipsPanel
          chips={chipList}
          selected={usable}
          onToggle={toggle}
          onCheck={() => check.mutate()}
          checking={check.isPending}
          checkBlocked={notReady}
        />
      )}
      {check.isError && (
        <p role="alert" className="text-sm text-destructive">
          {check.error instanceof ApiClientError ? check.error.message : "The chip check failed."}
        </p>
      )}
      {settings.isPending && <p className="text-sm text-muted-foreground">Reading settings…</p>}
      {settings.isError && (
        <p role="alert" className="text-sm text-destructive">
          {settings.error.message}
        </p>
      )}
      {settings.data && (
        <TestForm settings={settings.data} chips={usable} blocked={notReady} onStarted={setStartedId} />
      )}
    </div>
  );
}
