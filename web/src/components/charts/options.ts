import type { EChartsCoreOption } from "echarts/core";

import { closestIndex } from "@/components/charts/indexes";
import { formatHz } from "@/lib/format";
import type { ChipAnalysis } from "@/lib/generated/ChipAnalysis";
import type { Peak } from "@/lib/generated/Peak";
import type { PsdPeaks } from "@/lib/generated/PsdPeaks";
import type { ProbePoint } from "@/lib/generated/ProbePoint";
import type { Recommendation } from "@/lib/generated/Recommendation";
import type { Spectrogram } from "@/lib/generated/Spectrogram";
import type { Spectrum } from "@/lib/generated/Spectrum";
import type { SummedPsd } from "@/lib/generated/SummedPsd";
import { AXIS_LABEL, type Axis3 } from "@/pages/stream/axes";
import { chipColor } from "@/pages/stream/colors";
import { DEFAULT_CHART_THEME, type ChartTheme } from "@/ui-kit/chartTheme";
import { rampColors } from "@/components/charts/colormaps";
import { RAMP } from "@/stream/spectrogramScale";

import { BADGE_HALF, BADGE_PADDING, BADGE_WIDTH, base, splitLine, zip } from "./common";

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

/** How far apart, as a fraction of the frequency axis, two peak labels must be to sit on one level. */
const LABEL_GAP = 0.09;
/** How far each level lifts a peak label, in pixels. */
const LABEL_LIFT = 16;

/**
 * The level of each peak label, lowest first. A peak whose label would touch an earlier one (within
 * `gap` Hz) takes the lowest level those neighbours leave free, so labels in a crowd alternate.
 */
function labelLevels(freqs: number[], gap: number): number[] {
  const levels: number[] = [];
  for (const freq of freqs) {
    const taken = new Set(levels.filter((_, j) => freq - freqs[j] < gap));
    let level = 0;
    while (taken.has(level)) level += 1;
    levels.push(level);
  }
  return levels;
}

/** 1.05 times the largest total PSD up to `maxFreq`, as Shake&Tune scales its graph. */
function autoscale(chip: ChipAnalysis, maxFreq: number): number | undefined {
  let largest = Number.NEGATIVE_INFINITY;
  for (const [, data] of chip.datasets) {
    data.freq_bins.forEach((f, i) => {
      if (f <= maxFreq) largest = Math.max(largest, data.psd_sum[i]);
    });
  }
  return largest > 0 ? largest * 1.05 : undefined;
}

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
        { xAxis: best.freq, label: { formatter: `${best.name} ${formatHz(best.freq)} Hz`, color: theme.text } },
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
  const rowLabel = (label: string, top: number) => ({
    type: "text",
    left: 0,
    top: top + 4,
    style: { text: label, fill: theme.muted, fontSize: 12 },
  });
  return {
    ...base(null, maxFreq, theme),
    grid: { top: shapers.length > 0 ? 84 : 52, left: 64, right: 64, bottom: 40 },
    legend: [
      { type: "scroll", data: measured, top: 0, left: 56, selected, textStyle: { color: theme.text } },
      ...(shapers.length > 0
        ? [{ type: "scroll", data: shapers, top: 28, left: 56, textStyle: { color: theme.text } }]
        : []),
    ],
    graphic: [rowLabel("Axes", 0), ...(shapers.length > 0 ? [rowLabel("Shapers", 28)] : [])],
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
      left: 56,
      selected: { X: false, Y: false, Z: false },
      textStyle: { color: theme.text },
    },
    graphic: [{ type: "text", left: 0, top: 4, style: { text: "Axes", fill: theme.muted, fontSize: 12 } }],
    yAxis: { type: "value", name: "Vibration strength", splitLine: splitLine(theme) },
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
