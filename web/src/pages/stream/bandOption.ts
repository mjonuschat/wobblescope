import type { EChartsCoreOption } from "echarts/core";

import { type ChartTheme, DEFAULT_CHART_THEME } from "@/ui-kit/chartTheme";

export interface BandSeries {
  chip: string;
  color: string;
  /** Seconds before now (zero or negative) and the RMS vibration at that moment. */
  points: [number, number][];
}

/** One line per chip: the RMS vibration around the frequency of interest over the last `spanSeconds`. */
export function bandOption(
  series: BandSeries[],
  spanSeconds: number,
  theme: ChartTheme = DEFAULT_CHART_THEME,
): EChartsCoreOption {
  const splitLine = { lineStyle: { color: theme.grid } };
  return {
    animation: false,
    backgroundColor: "transparent",
    textStyle: { color: theme.text },
    tooltip: { trigger: "axis" },
    legend: series.length > 1 ? { type: "scroll", top: 0, textStyle: { color: theme.text } } : undefined,
    grid: { top: series.length > 1 ? 36 : 16, left: 64, right: 24, bottom: 36 },
    xAxis: { type: "value", name: "s", nameLocation: "middle", nameGap: 24, min: -spanSeconds, max: 0, splitLine },
    // Now is x = 0, the right end, where the y axis would otherwise sit; it stays on the left.
    yAxis: { type: "value", name: "mm/s² RMS", min: 0, splitLine, axisLine: { onZero: false } },
    series: series.map(({ chip, color, points }) => ({
      name: chip,
      type: "line",
      showSymbol: false,
      color,
      lineStyle: { color, width: 1.5 },
      data: points,
    })),
  };
}
