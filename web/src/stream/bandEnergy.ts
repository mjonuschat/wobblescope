import type { Spectrum } from "@/lib/generated/Spectrum";

/** How much history the energy strip keeps, in seconds. */
const HISTORY_SECONDS = 60;
/** How near a labelled peak a click has to be to pick that peak, in Hz. */
const SNAP_HZ = 3;

/**
 * The energy of the summed spectrum within `halfWidth` Hz either side of `centre`: the sum of
 * `psd_sum` over those bins times the bin spacing.
 */
export function bandEnergy(spectrum: Spectrum, centre: number, halfWidth: number): number {
  const { freq_bins: bins, psd_sum: power } = spectrum;
  if (bins.length < 2) return 0;
  const spacing = bins[1] - bins[0];
  let sum = 0;
  for (let i = 0; i < bins.length; i++) {
    if (Math.abs(bins[i] - centre) <= halfWidth + 1e-9) sum += power[i];
  }
  return sum * spacing;
}

/** The energy in the band over the last minute, per chip. */
export class BandHistory {
  private readonly points = new Map<string, { t: number; v: number }[]>();

  /** Adds a reading at `atSeconds` and drops those more than a minute older. */
  push(chip: string, atSeconds: number, value: number): void {
    const kept = (this.points.get(chip) ?? []).filter((p) => p.t >= atSeconds - HISTORY_SECONDS);
    kept.push({ t: atSeconds, v: value });
    this.points.set(chip, kept);
  }

  series(chip: string): { t: number; v: number }[] {
    return [...(this.points.get(chip) ?? [])];
  }

  reset(): void {
    this.points.clear();
  }
}

/**
 * The frequency a click on the spectrum picks: the nearest labelled peak within 3 Hz, else the
 * clicked frequency rounded to 0.1 Hz.
 */
export function pickFrequency(clicked: number, peaks: number[]): number {
  let nearest: number | null = null;
  for (const peak of peaks) {
    if (
      Math.abs(peak - clicked) <= SNAP_HZ &&
      (nearest === null || Math.abs(peak - clicked) < Math.abs(nearest - clicked))
    ) {
      nearest = peak;
    }
  }
  return nearest ?? Math.round(clicked * 10) / 10;
}
