import type { EChartsCoreOption } from "echarts/core";

import { rampColors } from "@/components/charts/colormaps";
import { closestIndex } from "@/components/charts/indexes";
import { formatHz } from "@/lib/format";
import type { PsdPeaks } from "@/lib/generated/PsdPeaks";
import type { Spectrogram } from "@/lib/generated/Spectrogram";
import { RAMP } from "@/stream/spectrogramScale";
import { DEFAULT_CHART_THEME, type ChartTheme } from "@/ui-kit/chartTheme";

import { BADGE_HALF, BADGE_PADDING, BADGE_WIDTH, LABEL_LIFT, base, labelLevels } from "./common";

const SPECTROGRAM_LABELS = 8;
/** Peak labels here carry the frequency, so they need more room than the PSD chart's. */
const SPECTROGRAM_LABEL_GAP = 0.12;

/** The spacing of the frequency ticks: round figures, with 25 Hz for the usual 200 Hz range. */
function tickStep(maxFreq: number): number {
  return maxFreq <= 250 ? 25 : maxFreq <= 500 ? 50 : 100;
}

/** The bin that carries the tick for each multiple of `step` Hz that has a bin within half a bin of it. */
function tickBins(freqs: number[], maxFreq: number): Map<number, number> {
  const bins = new Map<number, number>();
  if (freqs.length < 2) return bins;
  const half = (freqs[1] - freqs[0]) / 2;
  const step = tickStep(maxFreq);
  for (let multiple = 0; multiple <= maxFreq; multiple += step) {
    const bin = closestIndex(freqs, multiple);
    if (Math.abs(freqs[bin] - multiple) <= half) bins.set(bin, multiple);
  }
  return bins;
}

/**
 * The time-frequency map of one recording. Frequency runs across and the earliest time is at the bottom,
 * colour by log10 of the power from the floor to the maximum (a power below the floor takes the
 * floor's colour), and the PSD's peaks as dotted vertical lines.
 */
export function spectrogramOption(
  spectrogram: Spectrogram,
  peaks: PsdPeaks | null,
  maxFreq: number,
  theme: ChartTheme = DEFAULT_CHART_THEME,
): EChartsCoreOption {
  const { freq_bins: freqs, times, power, floor, max } = spectrogram;
  const lowest = Math.log10(floor);
  const data = power.flatMap((row, i) => row.map((p, j) => [j, i, Math.log10(Math.max(p, floor))]));
  const interval = (count: number) => Math.max(0, Math.ceil(count / SPECTROGRAM_LABELS) - 1);
  const ticks = tickBins(freqs, maxFreq);
  const oneDecimal = (value: string) => Number(value).toFixed(1);
  const marked = (peaks?.peaks ?? []).filter((peak) => peak.freq <= maxFreq);
  const levels = labelLevels(
    marked.map((peak) => peak.freq),
    maxFreq * SPECTROGRAM_LABEL_GAP,
  );
  const lifted = Math.max(0, ...levels) * LABEL_LIFT;
  return {
    ...base(null, maxFreq, theme),
    animation: false,
    grid: { top: 24 + lifted, left: 64, right: 96, bottom: 32 },
    tooltip: {
      formatter: (params: { data: number[] }) => {
        const [bin, row] = params.data;
        return `${times[row].toFixed(2)} s, ${freqs[bin].toFixed(1)} Hz<br/>power ${power[row][bin].toExponential(2)}`;
      },
    },
    xAxis: {
      type: "category",
      name: "Hz",
      data: freqs.map(String),
      axisTick: { interval: (index: number) => ticks.has(index), alignWithLabel: true },
      axisLabel: {
        interval: (index: number) => ticks.has(index),
        formatter: (_value: string, index: number) => String(ticks.get(index) ?? ""),
      },
      splitArea: { show: false },
    },
    yAxis: {
      type: "category",
      name: "Seconds",
      data: times.map(String),
      axisLabel: { interval: interval(times.length), formatter: oneDecimal },
      splitArea: { show: false },
    },
    visualMap: {
      type: "continuous",
      min: lowest,
      max: Math.log10(max),
      dimension: 2,
      calculable: true,
      orient: "vertical",
      right: 0,
      top: "middle",
      textStyle: { color: theme.text },
      formatter: (value: number) => (10 ** value).toExponential(0),
      inRange: { color: rampColors(RAMP) },
    },
    series: [
      {
        type: "heatmap",
        data,
        progressive: 0,
        ...(marked.length > 0
          ? {
              markLine: {
                silent: true,
                symbol: "none",
                lineStyle: { color: theme.series[1], type: "dotted", width: 1 },
                data: marked.map((peak, n) => ({
                  xAxis: closestIndex(freqs, peak.freq),
                  label: {
                    formatter: `{index|${n + 1}} {value|${formatHz(peak.freq)} Hz}`,
                    position: "end",
                    // The label starts at the line, so half the badge moves left of it and the badge sits on the line.
                    align: "left",
                    offset: [-BADGE_HALF, levels[n] === 0 ? 0 : -LABEL_LIFT * levels[n]],
                    rich: {
                      index: {
                        backgroundColor: theme.series[1],
                        color: theme.onFill(theme.series[1]),
                        fontWeight: "bold",
                        width: BADGE_WIDTH,
                        align: "center",
                        padding: [1, BADGE_PADDING],
                        borderRadius: 3,
                      },
                      value: { color: theme.text },
                    },
                  },
                })),
              },
            }
          : {}),
      },
    ],
  };
}
