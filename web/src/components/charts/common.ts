import type { EChartsCoreOption } from "echarts/core";

import type { ChartTheme } from "@/ui-kit/chartTheme";

/** Pairs each value with its bin. `xs` may be longer than `ys`: a shaper whose range ends lower has fewer values. */
export const zip = (xs: number[], ys: number[]) => ys.map((y, i) => [xs[i], y]);

export const splitLine = (theme: ChartTheme) => ({ lineStyle: { color: theme.grid } });
const palette = (theme: ChartTheme) => [theme.accent, ...theme.series];

export function base(title: string | null, maxFreq: number, theme: ChartTheme): EChartsCoreOption {
  return {
    backgroundColor: "transparent",
    textStyle: { color: theme.text },
    color: palette(theme),
    title: title
      ? { text: title, left: 0, textStyle: { color: theme.text, fontSize: 13, fontWeight: 500 } }
      : undefined,
    tooltip: { trigger: "axis" },
    grid: { top: title ? 92 : 64, left: 64, right: 64, bottom: 40 },
    xAxis: { type: "value", name: "Hz", min: 0, max: maxFreq, splitLine: splitLine(theme) },
  };
}

/** The number badge of a peak label is this wide, so one and two digits sit alike, and it is centred on the peak. */
export const BADGE_WIDTH = 18;
export const BADGE_PADDING = 5;
export const BADGE_HALF = (BADGE_WIDTH + 2 * BADGE_PADDING) / 2;

/** How far apart, as a fraction of the frequency axis, two peak labels must be to sit on one level. */
export const LABEL_GAP = 0.09;
/** How far each level lifts a peak label, in pixels. */
export const LABEL_LIFT = 16;

/**
 * The level of each peak label, lowest first. A peak whose label would touch an earlier one (within
 * `gap` Hz) takes the lowest level those neighbours leave free, so labels in a crowd alternate.
 */
export function labelLevels(freqs: number[], gap: number): number[] {
  const levels: number[] = [];
  for (const freq of freqs) {
    const taken = new Set(levels.filter((_, j) => freq - freqs[j] < gap));
    let level = 0;
    while (taken.has(level)) level += 1;
    levels.push(level);
  }
  return levels;
}
