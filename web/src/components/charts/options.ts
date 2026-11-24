import type { EChartsCoreOption } from "echarts/core";

import { formatHz } from "@/lib/format";
import type { ChipAnalysis } from "@/lib/generated/ChipAnalysis";
import type { Peak } from "@/lib/generated/Peak";
import type { ProbePoint } from "@/lib/generated/ProbePoint";
import type { Recommendation } from "@/lib/generated/Recommendation";
import type { Spectrum } from "@/lib/generated/Spectrum";
import type { SummedPsd } from "@/lib/generated/SummedPsd";
import { AXIS_LABEL, type Axis3 } from "@/pages/stream/axes";
import { chipColor } from "@/pages/stream/colors";
import { DEFAULT_CHART_THEME, type ChartTheme } from "@/ui-kit/chartTheme";

import {
  BADGE_HALF,
  BADGE_PADDING,
  BADGE_WIDTH,
  LABEL_GAP,
  LABEL_LIFT,
  base,
  labelLevels,
  splitLine,
  zip,
} from "./common";

/** Where a probe point is, or its number when its position is not known. */
export function pointLabel(index: number, probePoints: ProbePoint[]): string {
  const position = probePoints[index]?.position;
  return position ? `(${position.join(", ")})` : `point ${index}`;
}

interface PsdInput {
  theme?: ChartTheme;
  chip: ChipAnalysis;
  recommendation: Recommendation | null;
  /** The largest frequency the fit considered, in Hz. */
  maxFreq: number;
  /** The left axis ends here. Empty or 0 ends it at 1.05 times the largest total. */
  maxScale?: number | null;
}

/** 1.05 times the largest total PSD up to `maxFreq`, as Shake&Tune scales its graph. */
export function autoscale(chip: ChipAnalysis, maxFreq: number): number | undefined {
  let largest = Number.NEGATIVE_INFINITY;
  for (const [, data] of chip.datasets) {
    data.freq_bins.forEach((f, i) => {
      if (f <= maxFreq) largest = Math.max(largest, data.psd_sum[i]);
    });
  }
  return largest > 0 ? largest * 1.05 : undefined;
}

/** Where a legend row's items start, to the right of its switch (see `EChart`'s `legendSwitches`). */
export const LEGEND_LEFT = 92;

/**
 * One chip's PSDs on the left axis (the total per probe point, with X, Y and Z behind the
 * legend) and every fitted shaper's response on the right axis, with the recommended
 * frequency marked. Every curve is `core`'s array as returned.
 */
export function psdOption({
  chip,
  recommendation,
  maxFreq,
  maxScale,
  theme = DEFAULT_CHART_THEME,
}: PsdInput): EChartsCoreOption {
  const series: Record<string, unknown>[] = [];
  const selected: Record<string, boolean> = {};
  const many = chip.datasets.length > 1;
  const named = (point: number, what: string) => (many ? `Point ${point + 1} ${what}` : what);
  chip.datasets.forEach(([point, data], index) => {
    const total: Record<string, unknown> = {
      name: named(point, "Total"),
      type: "line",
      showSymbol: false,
      data: zip(data.freq_bins, data.psd_sum),
    };
    const found = chip.peaks.find(([p]) => p === point)?.[1];
    if (found) {
      const levels = labelLevels(
        found.peaks.map((p) => p.freq),
        maxFreq * LABEL_GAP,
      );
      total.markPoint = {
        silent: true,
        symbol: "circle",
        symbolSize: 6,
        itemStyle: { color: theme.text },
        data: found.peaks.map((peak, n) => ({
          coord: [peak.freq, peak.level],
          label: {
            formatter: `{index|${n + 1}} {value|${formatHz(peak.freq)} Hz}`,
            position: "top",
            // The label starts at the peak, so half the badge moves left of it and the badge sits on the peak.
            align: "left",
            offset: [-BADGE_HALF, levels[n] === 0 ? 0 : -LABEL_LIFT * levels[n]],
            color: peak.above_effect ? theme.danger : theme.text,
            fontWeight: peak.above_effect ? "bold" : "normal",
            fontSize: 13,
            // The number is a small badge, so it reads apart from the frequency beside it.
            rich: {
              index: {
                backgroundColor: peak.above_effect ? theme.danger : theme.muted,
                color: theme.onFill(peak.above_effect ? theme.danger : theme.muted),
                fontWeight: "bold",
                width: BADGE_WIDTH,
                align: "center",
                padding: [1, BADGE_PADDING],
                borderRadius: 3,
              },
              value: {
                color: peak.above_effect ? theme.danger : theme.text,
                fontWeight: peak.above_effect ? "bold" : "normal",
              },
            },
          },
        })),
      };
      if (index === 0) {
        const line = (yAxis: number) => ({
          yAxis,
          lineStyle: { color: theme.muted, type: "dashed", width: 1 },
          label: { show: false },
        });
        total.markLine = {
          silent: true,
          symbol: "none",
          data: [line(found.detect_threshold), line(found.effect_threshold)],
        };
        total.markArea = {
          silent: true,
          data: [
            [{ yAxis: 0, itemStyle: { color: theme.alpha(theme.success, 0.12) } }, { yAxis: found.detect_threshold }],
            [
              { yAxis: found.detect_threshold, itemStyle: { color: theme.alpha(theme.warning, 0.14) } },
              { yAxis: found.effect_threshold },
            ],
          ],
        };
      }
    }
    series.push(total);
    for (const [axis, values] of [
      ["X", data.psd_x],
      ["Y", data.psd_y],
      ["Z", data.psd_z],
    ] as const) {
      const name = named(point, axis);
      selected[name] = false;
      series.push({ name, type: "line", showSymbol: false, data: zip(data.freq_bins, values) });
    }
  });
  if (recommendation && series.length > 0) {
    const { best } = recommendation;
    const thresholds = (series[0].markLine as { data: unknown[] } | undefined)?.data ?? [];
    series[0].markLine = {
      silent: true,
      symbol: "none",
      data: [
        ...thresholds,
        {
          xAxis: best.freq,
          // At the bottom, below the axis numbers: at the top it lies over the marker of a peak at the same frequency.
          label: {
            formatter: `${best.name} ${formatHz(best.freq)} Hz`,
            color: theme.text,
            position: "start",
            offset: [0, 20],
          },
        },
      ],
    };
  }
  const grid = recommendation?.freq_bins ?? [];
  for (const candidate of recommendation?.all ?? []) {
    series.push({
      name: candidate.name,
      type: "line",
      yAxisIndex: 1,
      showSymbol: false,
      lineStyle: { type: "dashed" },
      data: zip(grid, candidate.vals),
    });
  }
  const measured = series.filter((s) => s.yAxisIndex === undefined).map((s) => s.name as string);
  const shapers = series.filter((s) => s.yAxisIndex === 1).map((s) => s.name as string);
  return {
    ...base(null, maxFreq, theme),
    grid: { top: shapers.length > 0 ? 84 : 52, left: 64, right: 64, bottom: 40 },
    legend: [
      { type: "scroll", data: measured, top: 0, left: LEGEND_LEFT, selected, textStyle: { color: theme.text } },
      ...(shapers.length > 0
        ? [{ type: "scroll", data: shapers, top: 28, left: LEGEND_LEFT, textStyle: { color: theme.text } }]
        : []),
    ],
    yAxis: [
      {
        type: "value",
        name: "Vibration strength",
        splitLine: splitLine(theme),
        min: 0,
        max: maxScale && maxScale > 0 ? maxScale : autoscale(chip, maxFreq),
        axisLabel: {
          formatter: (value: number) =>
            Number(value.toPrecision(3)).toLocaleString("en-US", { maximumFractionDigits: 20 }),
        },
      },
      {
        type: "value",
        name: "Remaining vibration",
        min: 0,
        max: 1,
        splitLine: { show: false },
        // The shaper's response is a fraction: 1 lets all the vibration through, 0 none of it.
        axisLabel: { formatter: (value: number) => `${Math.round(value * 100)}%` },
      },
    ],
    series,
  };
}

export function summedOption(
  summed: SummedPsd,
  maxFreq: number,
  theme: ChartTheme = DEFAULT_CHART_THEME,
  /** Where the vibration axis ends; the data's own maximum when omitted. */
  max?: number,
): EChartsCoreOption {
  const curves = [
    ["Total", summed.psd],
    ["X", summed.psd_x],
    ["Y", summed.psd_y],
    ["Z", summed.psd_z],
  ] as const;
  return {
    ...base(null, maxFreq, theme),
    // One axis, so the plot reaches the right edge. The legend row sits above it.
    grid: { top: 64, left: 64, right: 24, bottom: 40 },
    legend: {
      type: "scroll",
      data: curves.map(([name]) => name),
      top: 0,
      left: LEGEND_LEFT,
      selected: { X: false, Y: false, Z: false },
      textStyle: { color: theme.text },
    },
    yAxis: { type: "value", name: "Vibration strength", splitLine: splitLine(theme), max },
    series: curves.map(([name, values]) => ({
      name,
      type: "line",
      showSymbol: false,
      data: zip(summed.freq_bins, values),
    })),
  };
}

interface LiveSpectrumInput {
  theme?: ChartTheme;
  /** `colorIndex` is the chip's place in the page's selection, so its colour matches the other charts. */
  chips: { chip: string; colorIndex?: number; spectrum: Spectrum; peaks: Peak[]; peakHold: number[]; axes: Axis3[] }[];
  maxHz: number;
  /** The frequency to mark with a vertical line, in Hz. */
  frequencyOfInterest?: number | null;
}

const SPECTRUM_VALUES = {
  x: (s: Spectrum) => s.psd_x,
  y: (s: Spectrum) => s.psd_y,
  z: (s: Spectrum) => s.psd_z,
  sum: (s: Spectrum) => s.psd_sum,
} as const;

/**
 * The live spectra. One line per chip and chosen axis in the chip's colour, a faint peak-hold
 * line per chip (the summed spectrum's hold) and the chip's peaks marked on its first line.
 */
export function liveSpectrumOption({
  chips,
  maxHz,
  frequencyOfInterest,
  theme = DEFAULT_CHART_THEME,
}: LiveSpectrumInput): EChartsCoreOption {
  const series: Record<string, unknown>[] = [];
  chips.forEach(({ chip, colorIndex, spectrum, peaks, peakHold, axes }, index) => {
    const color = chipColor(colorIndex ?? index, theme);
    axes.forEach((axis, n) => {
      const line: Record<string, unknown> = {
        name: `${chip} ${AXIS_LABEL[axis]}`,
        type: "line",
        showSymbol: false,
        color,
        lineStyle: { color, width: 1.5, type: axis === "sum" ? "solid" : axis === "x" ? "dashed" : "dotted" },
        data: zip(spectrum.freq_bins, SPECTRUM_VALUES[axis](spectrum)),
      };
      if (n === 0) {
        line.markPoint = {
          silent: true,
          symbol: "circle",
          symbolSize: 6,
          itemStyle: { color },
          data: peaks.map((peak) => ({
            coord: [peak.freq, peak.level],
            value: peak.level,
            label: {
              formatter: `${formatHz(peak.freq)} Hz\n${peak.level.toExponential(1)}`,
              position: "top",
              color: theme.text,
              fontSize: 10,
            },
          })),
        };
      }
      if (n === 0 && index === 0 && frequencyOfInterest != null) {
        line.markLine = {
          silent: true,
          symbol: "none",
          lineStyle: { color: theme.text, type: "dashed", width: 1 },
          label: { formatter: `${formatHz(frequencyOfInterest)} Hz`, color: theme.text, fontSize: 10 },
          data: [{ xAxis: frequencyOfInterest }],
        };
      }
      series.push(line);
    });
    series.push({
      name: `${chip} Peak Hold`,
      type: "line",
      showSymbol: false,
      color,
      lineStyle: { color, width: 1, opacity: 0.3 },
      data: zip(spectrum.freq_bins, peakHold),
    });
  });
  return {
    animation: false,
    ...base(null, maxHz, theme),
    grid: { top: 64, left: 64, right: 24, bottom: 40 },
    legend: { type: "scroll", top: 0, left: 56, textStyle: { color: theme.text } },
    yAxis: { type: "value", name: "PSD", splitLine: splitLine(theme) },
    series,
  };
}
