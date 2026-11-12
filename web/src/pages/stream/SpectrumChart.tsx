import { useCallback, useMemo } from "react";

import { EChart } from "@/components/charts/EChart";
import { liveSpectrumOption } from "@/components/charts/options";
import { pickFrequency } from "@/stream/pickFrequency";

type Chips = Parameters<typeof liveSpectrumOption>[0]["chips"];

interface Props {
  chips: Chips;
  maxHz: number;
  /** The frequency to mark, in Hz. */
  frequencyOfInterest: number | null;
  /** Called with the frequency a click picks: a labelled peak near it, else the clicked one. */
  onPick: (hz: number) => void;
}

/** The live spectra of the chips. */
export function SpectrumChart({ chips, maxHz, frequencyOfInterest, onPick }: Props) {
  const option = useMemo(
    () => liveSpectrumOption({ chips, maxHz, frequencyOfInterest }),
    [chips, maxHz, frequencyOfInterest],
  );
  const peaks = useMemo(() => chips.flatMap((c) => c.peaks.map((p) => p.freq)), [chips]);
  const click = useCallback((x: number) => onPick(pickFrequency(x, peaks)), [onPick, peaks]);
  return <EChart option={option} label="Live spectrum of the streamed chips" height={420} onClickX={click} />;
}
