import { useRef } from "react";

import type { Spectrum } from "@/lib/generated/Spectrum";
import { bandRms } from "@/stream/bandEnergy";

/** How much time the strip shows, in milliseconds. It matches the spectrogram. */
export const BAND_SPAN_MS = 60_000;

export interface BandPoint {
  /** The clock reading when the spectrum arrived, in milliseconds. */
  at: number;
  /** The band's RMS vibration at that moment. */
  rms: number;
}

/**
 * The RMS vibration around the frequency of interest, per chip, over the last minute. Each new
 * spectrum adds a point measured around the frequency of interest as it was then, so a hold that
 * ramps leaves a strip that follows it. Nothing is recorded while there is no frequency of
 * interest, and a chip that is no longer shown is forgotten.
 */
export function useBandHistory(
  spectra: Record<string, Spectrum>,
  centerHz: number | null,
  halfWidthHz: number,
  now: () => number = () => performance.now(),
): Record<string, BandPoint[]> {
  const history = useRef(new Map<string, BandPoint[]>());
  const seen = useRef(new Map<string, Spectrum>());
  // Adding a point only when the spectrum is a new object keeps this safe to run twice per render.
  const at = now();
  for (const chip of [...history.current.keys()]) {
    if (!(chip in spectra)) {
      history.current.delete(chip);
      seen.current.delete(chip);
    }
  }
  for (const [chip, spectrum] of Object.entries(spectra)) {
    const points = history.current.get(chip) ?? [];
    if (centerHz !== null && seen.current.get(chip) !== spectrum) {
      points.push({ at, rms: bandRms(spectrum, centerHz, halfWidthHz) });
    }
    seen.current.set(chip, spectrum);
    const first = points.findIndex((p) => at - p.at <= BAND_SPAN_MS);
    history.current.set(chip, first === -1 ? [] : points.slice(first));
  }
  return Object.fromEntries(history.current);
}
