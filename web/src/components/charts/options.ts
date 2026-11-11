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

/** Where a probe point is, or its number when its position is not known. */
export function pointLabel(index: number, probePoints: ProbePoint[]): string {
  const position = probePoints[index]?.position;
  return position ? `(${position.join(", ")})` : `point ${index}`;
}

/** Pairs each value with its bin. `xs` may be longer than `ys`: a shaper whose range ends lower has fewer values. */
const zip = (xs: number[], ys: number[]) => ys.map((y, i) => [xs[i], y]);

const text = "#d4d4d4";
const MUTED = "#a3a3a3";
const GRID = "rgba(255, 255, 255, 0.08)";
const PALETTE = ["#ff7a33", "#4da3ff", "#7ee081", "#ffd166", "#c792ea", "#ef5b7b", "#5eead4", "#f59e0b"];
const splitLine = { lineStyle: { color: GRID } };

function base(title: string | null, maxFreq: number): EChartsCoreOption {
  return {
    backgroundColor: "transparent",
    textStyle: { color: text },
    color: PALETTE,
    title: title ? { text: title, left: 0, textStyle: { color: text, fontSize: 13, fontWeight: 500 } } : undefined,
    tooltip: { trigger: "axis" },
    grid: { top: title ? 92 : 64, left: 64, right: 64, bottom: 40 },
    xAxis: { type: "value", name: "Hz", min: 0, max: maxFreq, splitLine },
  };
}

interface PsdInput {
  chip: ChipAnalysis;
  recommendation: Recommendation | null;
  /** The largest frequency the fit considered, in Hz. */
  maxFreq: number;
}

/**
 * One chip's PSDs on the left axis (the total per probe point, with X, Y and Z behind the
 * legend) and every fitted shaper's response on the right axis, with the recommended
 * frequency marked. Every curve is `core`'s array as returned.
 */
export function psdOption({ chip, recommendation, maxFreq }: PsdInput): EChartsCoreOption {
  const series: Record<string, unknown>[] = [];
  const selected: Record<string, boolean> = {};
  const many = chip.datasets.length > 1;
  const named = (point: number, what: string) => (many ? `Point ${point + 1} ${what}` : what);
  for (const [point, data] of chip.datasets) {
    series.push({
      name: named(point, "Total"),
      type: "line",
      showSymbol: false,
      data: zip(data.freq_bins, data.psd_sum),
    });
    for (const [axis, values] of [
      ["X", data.psd_x],
      ["Y", data.psd_y],
      ["Z", data.psd_z],
    ] as const) {
      const name = named(point, axis);
      selected[name] = false;
      series.push({ name, type: "line", showSymbol: false, data: zip(data.freq_bins, values) });
    }
  }
  if (recommendation && series.length > 0) {
    const { best } = recommendation;
    series[0].markLine = {
      silent: true,
      symbol: "none",
      data: [{ xAxis: best.freq, label: { formatter: `${best.name} ${formatHz(best.freq)} Hz`, color: text } }],
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
    style: { text: label, fill: MUTED, fontSize: 12 },
  });
  return {
    ...base(null, maxFreq),
    grid: { top: shapers.length > 0 ? 84 : 52, left: 64, right: 64, bottom: 40 },
    legend: [
      { data: measured, top: 0, left: 56, selected, textStyle: { color: text } },
      ...(shapers.length > 0 ? [{ data: shapers, top: 28, left: 56, textStyle: { color: text } }] : []),
    ],
    graphic: [rowLabel("Axes", 0), ...(shapers.length > 0 ? [rowLabel("Shapers", 28)] : [])],
    yAxis: [
      { type: "value", name: "Vibration Strength", splitLine },
      {
        type: "value",
        name: "Remaining Vibration",
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

export function summedOption(summed: SummedPsd, maxFreq: number): EChartsCoreOption {
  const curves = [
    ["Total", summed.psd],
    ["X", summed.psd_x],
    ["Y", summed.psd_y],
    ["Z", summed.psd_z],
  ] as const;
  return {
    ...base(null, maxFreq),
    // One axis, so the plot reaches the right edge. The legend row sits above it.
    grid: { top: 64, left: 64, right: 24, bottom: 40 },
    legend: {
      data: curves.map(([name]) => name),
      top: 0,
      left: 56,
      selected: { X: false, Y: false, Z: false },
      textStyle: { color: text },
    },
    graphic: [{ type: "text", left: 0, top: 4, style: { text: "Axes", fill: MUTED, fontSize: 12 } }],
    yAxis: { type: "value", name: "Vibration Strength", splitLine },
    series: curves.map(([name, values]) => ({
      name,
      type: "line",
      showSymbol: false,
      data: zip(summed.freq_bins, values),
    })),
  };
}

interface LiveSpectrumInput {
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
export function liveSpectrumOption({ chips, maxHz, frequencyOfInterest }: LiveSpectrumInput): EChartsCoreOption {
  const series: Record<string, unknown>[] = [];
  chips.forEach(({ chip, colorIndex, spectrum, peaks, peakHold, axes }, index) => {
    const color = chipColor(colorIndex ?? index);
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
          symbol: "pin",
          symbolSize: 36,
          itemStyle: { color },
          data: peaks.map((peak) => ({
            coord: [peak.freq, peak.level],
            value: peak.level,
            label: {
              formatter: `${formatHz(peak.freq)} Hz\n${peak.level.toExponential(1)}`,
              color: text,
              fontSize: 10,
            },
          })),
        };
      }
      if (n === 0 && index === 0 && frequencyOfInterest != null) {
        line.markLine = {
          silent: true,
          symbol: "none",
          lineStyle: { color: text, type: "dashed", width: 1 },
          label: { formatter: `${formatHz(frequencyOfInterest)} Hz`, color: text, fontSize: 10 },
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
    ...base(null, maxHz),
    grid: { top: 64, left: 64, right: 24, bottom: 40 },
    legend: { type: "scroll", top: 0, left: 56, textStyle: { color: text } },
    yAxis: { type: "value", name: "PSD", splitLine },
    series,
  };
}

interface BandEnergyInput {
  /** Each chip's energy readings, with `t` in seconds on one clock. */
  chips: { chip: string; color: string; points: { t: number; v: number }[] }[];
  centre: number;
  halfWidth: number;
}

/** The energy in a band around one frequency over the last minute, one line per chip. */
export function bandEnergyOption({ chips, centre, halfWidth }: BandEnergyInput): EChartsCoreOption {
  const latest = Math.max(0, ...chips.flatMap((c) => c.points.map((p) => p.t)));
  return {
    backgroundColor: "transparent",
    textStyle: { color: text },
    tooltip: { trigger: "axis" },
    grid: { top: 24, left: 64, right: 24, bottom: 40 },
    xAxis: { type: "value", name: "s", min: -60, max: 0, splitLine },
    yAxis: {
      type: "value",
      name: `Energy ${formatHz(centre - halfWidth)} to ${formatHz(centre + halfWidth)} Hz`,
      splitLine,
    },
    series: chips.map(({ chip, color, points }) => ({
      name: chip,
      type: "line",
      showSymbol: false,
      color,
      lineStyle: { color, width: 1.5 },
      data: points.map((p) => [p.t - latest, p.v]),
    })),
  };
}
