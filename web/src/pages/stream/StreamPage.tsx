import { useEffect, useRef, useState } from "react";

import { Panel } from "@/components/Panel";
import { Waiting } from "@/components/Waiting";
import { useChips } from "@/lib/api/queries";
import { type SpectraState, type SpectrumWorkerLike, useSpectra } from "@/stream/useSpectra";
import { useStream } from "@/stream/useStream";

import type { Axis3 } from "./axes";
import { chipColor } from "./colors";
import { ChipPicker } from "./ChipPicker";
import { ConsoleCard } from "./ConsoleCard";
import { DisplaySettings, type Range } from "./DisplaySettings";
import { HealthBadge } from "./HealthBadge";
import { SignalStrips } from "./SignalStrips";
import { Spectrogram } from "./Spectrogram";
import { SpectrumChart } from "./SpectrumChart";
import { useSignalBuffers } from "./useSignalBuffers";

interface Props {
  /** Tests pass a fake spectrum worker. */
  createWorker?: () => SpectrumWorkerLike;
}

const DEFAULT_MAX_CHIPS = 4;
const DEFAULT_WINDOW_SECONDS = 1;

/** The Live View page shows live signals and spectra of up to a few chips. It is read only. */
export function StreamPage({ createWorker }: Props) {
  const stream = useStream();
  const chipList = useChips().data;
  const { hello, chips: streams } = stream.state;
  const { subscribe } = stream;
  const [selected, setSelected] = useState<string[]>([]);
  const [axes, setAxes] = useState<Record<string, Axis3[]>>({});
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

  // "Full" is the daemon's limit or half the slowest active chip's sample rate, whichever is less.
  const limit = hello?.limits.max_hz ?? 0;
  const rates = selected.flatMap((section) => {
    const rate = streams[section]?.inputRateHz || chipList?.chips.find((c) => c.section === section)?.nominal_rate_hz;
    return rate ? [rate] : [];
  });
  const fullHz = rates.length > 0 ? Math.min(limit, Math.floor(Math.min(...rates) / 2)) : limit;
  const defaultMaxHz = hello?.defaults.max_hz ?? 0;
  const effectiveRange = range ?? defaultMaxHz;
  const maxHz = effectiveRange === "full" ? fullHz : effectiveRange;

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
        <Panel title="Display">
          <DisplaySettings
            updatesPerSecond={updatesPerSecond}
            rateLimit={hello.limits.max_updates_per_second}
            onUpdatesPerSecond={setUpdates}
            range={effectiveRange}
            defaultMaxHz={defaultMaxHz}
            fullHz={fullHz}
            onRange={setRange}
            windowSeconds={windowSeconds}
            onWindowSeconds={setWindowSeconds}
            paused={paused}
            onPaused={pause}
            onResetPeakHold={() => resetPeakHold()}
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
            axes={axes}
            onToggle={toggle}
            onAxes={(section, next) => setAxes((old) => ({ ...old, [section]: next }))}
            badge={(section) => (
              <HealthBadge chip={section} state={streams[section]} error={spectra[section]?.error ?? null} />
            )}
          />
        </Panel>
      )}
      {chipList && !hello && <Waiting>Waiting for the daemon's defaults.</Waiting>}
      {selected.length === 0 && <p className="text-sm text-muted-foreground">Pick a chip to see its signal.</p>}
      {selected.length > 0 && (
        <Panel title="Signals">
          <SignalStrips
            paused={paused}
            chips={selected.map((chip, i) => ({ chip, color: chipColor(i), signal: shownSignals[chip] }))}
          />
        </Panel>
      )}
      {shown.length > 0 && (
        <Panel title="Live spectrum">
          <SpectrumChart
            maxHz={maxHz}
            onResetPeakHold={() => resetPeakHold()}
            chips={shown.map(({ chip, index, entry, spectrum }) => ({
              chip,
              colorIndex: index,
              spectrum,
              peaks: entry.peaks,
              peakHold: entry.peakHold,
              axes: axes[chip] ?? ["sum"],
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
