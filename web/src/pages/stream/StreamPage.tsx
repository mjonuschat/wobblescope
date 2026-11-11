import { useEffect, useRef, useState } from "react";

import { AudioLinesIcon } from "lucide-react";

import { Panel } from "@/components/Panel";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { Waiting } from "@/components/Waiting";
import { useChips } from "@/lib/api/queries";
import { type SpectraState, type SpectrumWorkerLike, useSpectra } from "@/stream/useSpectra";
import { useStream } from "@/stream/useStream";

import type { Axis3 } from "./axes";
import { AxisToggles } from "./AxisToggles";
import { chipColor } from "./colors";
import { ChipPicker } from "./ChipPicker";
import { ConsoleCard } from "./ConsoleCard";
import { DisplaySettings, PauseButton, type Range } from "./DisplaySettings";
import { HealthBadge } from "./HealthBadge";
import { HoldPanel } from "./HoldPanel";
import { SignalStrips } from "./SignalStrips";
import { Spectrogram } from "./Spectrogram";
import { SpectrumChart } from "./SpectrumChart";
import { useHold } from "./useHold";
import { useSignalBuffers } from "./useSignalBuffers";

interface Props {
  /** Tests pass a fake spectrum worker. */
  createWorker?: () => SpectrumWorkerLike;
}

const DEFAULT_MAX_CHIPS = 4;
const DEFAULT_WINDOW_SECONDS = 1;

/** The Live View page shows live signals and spectra of up to a few chips, and can hold the printer at a frequency. */
export function StreamPage({ createWorker }: Props) {
  const stream = useStream();
  const hold = useHold();
  const chipList = useChips().data;
  const { hello, chips: streams } = stream.state;
  const { subscribe } = stream;
  const [selected, setSelected] = useState<string[]>([]);
  const [axes, setAxes] = useState<Axis3[]>(["sum"]);
  const [range, setRange] = useState<Range | null>(null);
  const [updates, setUpdates] = useState<number | null>(null);
  const [windowSeconds, setWindowSeconds] = useState(DEFAULT_WINDOW_SECONDS);
  const [paused, setPaused] = useState(false);

  const updatesPerSecond = updates ?? hello?.defaults.updates_per_second ?? 1;
  const { spectra, resetPeakHold } = useSpectra(stream, windowSeconds, updatesPerSecond, createWorker, paused);
  const signals = useSignalBuffers(stream);

  // A paused view keeps the data it had. The socket keeps running and the worker is not fed while paused.
  const [frozen, setFrozen] = useState<{ spectra: SpectraState; signals: typeof signals } | null>(null);
  const shownSpectra = frozen?.spectra ?? spectra;
  const shownSignals = frozen?.signals ?? signals;
  function pause(value: boolean) {
    setPaused(value);
    setFrozen(value ? { spectra, signals } : null);
  }

  // The daemon clamps each chip's request to what that chip can show, so "Max" sends its own limit.
  const defaultMaxHz = hello?.defaults.max_hz ?? 0;
  const effectiveRange = range ?? defaultMaxHz;
  const maxHz = effectiveRange === "max" ? (hello?.limits.max_hz ?? 0) : effectiveRange;
  const announcedHz = Math.max(0, ...selected.map((section) => streams[section]?.maxHz ?? 0));
  const axisHz = announcedHz || maxHz;

  // Changing a setting sends Subscribe again for the active chips.
  const previous = useRef<string | null>(null);
  useEffect(() => {
    if (!hello) return;
    const key = `${maxHz}/${updatesPerSecond}`;
    if (previous.current !== null && previous.current !== key) {
      for (const section of selected) subscribe(section, maxHz, updatesPerSecond);
    }
    previous.current = key;
  }, [hello, maxHz, updatesPerSecond, selected, subscribe]);

  const maxChips = hello?.limits.max_chips ?? DEFAULT_MAX_CHIPS;

  function toggle(section: string) {
    if (selected.includes(section)) {
      setSelected(selected.filter((s) => s !== section));
      stream.unsubscribe(section);
    } else if (selected.length < maxChips) {
      setSelected([...selected, section]);
      stream.subscribe(section, maxHz, updatesPerSecond);
    }
  }

  // Turns on every chip Klipper reports as usable, up to the most that can stream at once.
  function enableAll() {
    const sections = (chipList?.chips ?? [])
      .filter((chip) => chip.availability.state !== "unavailable")
      .map((chip) => chip.section)
      .slice(0, maxChips);
    setSelected(sections);
    for (const section of sections) stream.subscribe(section, maxHz, updatesPerSecond);
  }

  const shown = selected.flatMap((chip, index) => {
    const entry = shownSpectra[chip];
    return entry?.spectrum ? [{ chip, index, entry, spectrum: entry.spectrum }] : [];
  });

  if (!stream.state.connected) {
    return (
      <section className="mx-auto max-w-5xl space-y-4">
        <h1 className="text-xl font-semibold">Live View</h1>
        <p className="text-sm text-muted-foreground">Not connected to the daemon. The stream starts when it is.</p>
      </section>
    );
  }

  return (
    <section className="mx-auto max-w-5xl space-y-4">
      <h1 className="text-xl font-semibold">Live View</h1>
      {hello?.printing && <p className="text-sm text-amber-400">A print is running: showing a lower update rate</p>}
      {hello && (
        <Panel title="Settings" action={<PauseButton paused={paused} onPaused={pause} />}>
          <DisplaySettings
            updatesPerSecond={updatesPerSecond}
            rateLimit={hello.limits.max_updates_per_second}
            onUpdatesPerSecond={setUpdates}
            range={effectiveRange}
            defaultMaxHz={defaultMaxHz}
            onRange={setRange}
            windowSeconds={windowSeconds}
            onWindowSeconds={setWindowSeconds}
          />
        </Panel>
      )}
      {chipList && chipList.chips.length === 0 && (
        <p className="text-sm text-muted-foreground">Klipper's config lists no supported accelerometer.</p>
      )}
      {chipList && hello && (
        <Panel title="Chips">
          <ChipPicker
            chips={chipList.chips}
            selected={selected}
            maxChips={maxChips}
            onToggle={toggle}
            badge={(section) => (
              <HealthBadge chip={section} state={streams[section]} error={spectra[section]?.error ?? null} />
            )}
          />
        </Panel>
      )}
      <HoldPanel hold={hold} frequencyOfInterest={null} />
      {chipList && !hello && <Waiting>Waiting for the daemon's defaults.</Waiting>}
      {hello && chipList && chipList.chips.length > 0 && selected.length === 0 && (
        <Card>
          <CardContent>
            <Empty>
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <AudioLinesIcon />
                </EmptyMedia>
                <EmptyTitle>No Sources Enabled</EmptyTitle>
                <EmptyDescription>Please enable at least one chip to stream data from.</EmptyDescription>
              </EmptyHeader>
              <EmptyContent>
                <Button type="button" variant="outline" onClick={enableAll}>
                  Enable All
                </Button>
              </EmptyContent>
            </Empty>
          </CardContent>
        </Card>
      )}
      {selected.length > 0 && (
        <Panel title="Signals">
          <SignalStrips
            paused={paused}
            chips={selected.map((chip, i) => ({ chip, color: chipColor(i), signal: shownSignals[chip] }))}
          />
        </Panel>
      )}
      {shown.length > 0 && (
        <Panel
          title="Live Spectrum"
          action={
            <div className="flex items-center gap-2">
              <AxisToggles axes={axes} onAxes={setAxes} />
              <Button type="button" variant="outline" size="sm" onClick={() => resetPeakHold()}>
                Reset Peak Hold
              </Button>
            </div>
          }
        >
          <SpectrumChart
            maxHz={axisHz}
            chips={shown.map(({ chip, index, entry, spectrum }) => ({
              chip,
              colorIndex: index,
              spectrum,
              peaks: entry.peaks,
              peakHold: entry.peakHold,
              axes,
            }))}
          />
        </Panel>
      )}
      {shown.length > 0 && (
        <Panel title="Spectrogram">
          {shown.map(({ chip, index, spectrum }) => (
            <Spectrogram key={chip} chip={chip} color={chipColor(index)} spectrum={spectrum} />
          ))}
        </Panel>
      )}
      <ConsoleCard lines={stream.state.console} />
    </section>
  );
}
