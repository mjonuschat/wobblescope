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
