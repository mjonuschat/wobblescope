import { useEffect, useMemo, useRef, useState } from "react";

import { EChart } from "@/components/charts/EChart";
import { bandEnergyOption } from "@/components/charts/options";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { Spectrum } from "@/lib/generated/Spectrum";
import { BandHistory, bandEnergy } from "@/stream/bandEnergy";

const DEFAULT_HALF_WIDTH_HZ = 2;

interface Props {
  chips: { chip: string; color: string; spectrum: Spectrum }[];
  /** The frequency whose band is shown, in Hz. */
  frequencyOfInterest: number;
  /** The clock in seconds. Tests pass their own. */
  now?: () => number;
}

/** The energy in a band around the frequency of interest over the last minute, one line per chip. */
export function EnergyStrip({ chips, frequencyOfInterest, now = () => performance.now() / 1000 }: Props) {
  const [halfWidth, setHalfWidth] = useState(DEFAULT_HALF_WIDTH_HZ);
  const history = useRef(new BandHistory());
  const band = useRef("");
  const [readings, setReadings] = useState<Record<string, { t: number; v: number }[]>>({});

  // A reading is taken when a new spectrum arrives or the band changes, not on every render.
  const seen = useRef<Spectrum[]>([]);
  useEffect(() => {
    const key = `${frequencyOfInterest}/${halfWidth}`;
    const spectra = chips.map((c) => c.spectrum);
    const same = seen.current.length === spectra.length && spectra.every((s, i) => s === seen.current[i]);
    if (same && band.current === key) return;
    if (band.current !== key) {
      history.current.reset();
      band.current = key;
    }
    seen.current = spectra;
    const t = now();
    for (const { chip, spectrum } of chips) {
      history.current.push(chip, t, bandEnergy(spectrum, frequencyOfInterest, halfWidth));
    }
    setReadings(Object.fromEntries(chips.map(({ chip }) => [chip, history.current.series(chip)])));
  });

  const option = useMemo(
    () =>
      bandEnergyOption({
        chips: chips.map(({ chip, color }) => ({ chip, color, points: readings[chip] ?? [] })),
        centre: frequencyOfInterest,
        halfWidth,
      }),
    [chips, readings, frequencyOfInterest, halfWidth],
  );

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <Label htmlFor="band-half-width">Band half width (Hz)</Label>
        <Input
          id="band-half-width"
          type="number"
          min={0.5}
          step={0.5}
          className="w-24"
          value={halfWidth}
          onChange={(e) => {
            const value = Number(e.target.value);
            if (Number.isFinite(value) && value > 0) setHalfWidth(value);
          }}
        />
      </div>
      <EChart option={option} label="Energy around the frequency of interest" height={200} />
    </div>
  );
}
