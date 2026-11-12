/** How much history the auto scale looks at, in milliseconds. */
const SPAN_MS = 60_000;
/** Bins kept per spectrum for the percentiles. That is enough for them and cheap to sort. */
const SAMPLES_PER_SPECTRUM = 32;
/** The auto scale is recomputed once per this many spectra from each chip. */
const SCALE_EVERY = 10;

/** The colour scale in log10 power: `low` is the dark end and `low + range` the pale end. */
export interface ColourScale {
  low: number;
  range: number;
}

/** Where the spectrograms get their colour scale from. One source serves every chip shown. */
export interface ScaleSource {
  /** Takes a chip's new spectrum, as log10 power, and returns the scale to draw its column with. */
  columnScale(chip: string, at: number, logPsd: number[]): ColourScale;
  /** The scale in use now, for the colour bar. */
  current(): ColourScale;
}

/** The value below which `fraction` of the sorted values lie. */
function percentile(sorted: number[], fraction: number): number {
  return sorted[Math.min(sorted.length - 1, Math.floor(fraction * sorted.length))];
}

/**
 * One scale from the 5th to the 99th percentile of the last minute of every chip's spectra, so
 * the same colour means the same power on every chip. It drifts as the data does.
 */
export class SharedAutoScale implements ScaleSource {
  private readonly samples = new Map<string, { at: number; values: number[] }[]>();
  private since = 0;
  private scale: ColourScale = { low: 0, range: 1 };
  private computed = false;

  columnScale(chip: string, at: number, logPsd: number[]): ColourScale {
    const stride = Math.max(1, Math.ceil(logPsd.length / SAMPLES_PER_SPECTRUM));
    const kept = (this.samples.get(chip) ?? []).filter((sample) => at - sample.at <= SPAN_MS);
    kept.push({ at, values: logPsd.filter((_, bin) => bin % stride === 0) });
    this.samples.set(chip, kept);
    if (!this.computed || ++this.since >= SCALE_EVERY * this.samples.size) this.recompute();
    return this.scale;
  }

  current(): ColourScale {
    return this.scale;
  }

  private recompute() {
    const all = [...this.samples.values()]
      .flat()
      .flatMap((sample) => sample.values)
      .sort((a, b) => a - b);
    if (all.length === 0) return;
    const low = percentile(all, 0.05);
    this.scale = { low, range: Math.max(percentile(all, 0.99) - low, 1e-9) };
    this.computed = true;
    this.since = 0;
  }
}

/** A scale from two powers (not their logarithms), the same for every chip and every moment. */
export function fixedScale(lowPsd: number, highPsd: number): ScaleSource {
  const low = Math.log10(lowPsd);
  const scale = { low, range: Math.max(Math.log10(highPsd) - low, 1e-9) };
  return { columnScale: () => scale, current: () => scale };
}

/** Dark blue, through purple and orange, to pale yellow. */
export const RAMP: [number, number, number][] = [
  [10, 10, 60],
  [90, 20, 120],
  [200, 60, 60],
  [250, 170, 40],
  [255, 250, 200],
];
