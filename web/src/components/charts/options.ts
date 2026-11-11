import type { EChartsCoreOption } from "echarts/core";

import { formatHz } from "@/lib/format";
import type { ChipAnalysis } from "@/lib/generated/ChipAnalysis";
import type { ProbePoint } from "@/lib/generated/ProbePoint";
import type { Recommendation } from "@/lib/generated/Recommendation";
import type { SummedPsd } from "@/lib/generated/SummedPsd";

/** Where a probe point is, or its number when its position is not known. */
export function pointLabel(index: number, probePoints: ProbePoint[]): string {
  const position = probePoints[index]?.position;
  return position ? `(${position.join(", ")})` : `point ${index}`;
}

/** Pairs each value with its bin. `xs` may be longer than `ys`: a shaper whose range ends lower has fewer values. */
const zip = (xs: number[], ys: number[]) => ys.map((y, i) => [xs[i], y]);

const text = "#d4d4d4";
const GRID = "rgba(255, 255, 255, 0.08)";
const PALETTE = ["#ff7a33", "#4da3ff", "#7ee081", "#ffd166", "#c792ea", "#ef5b7b", "#5eead4", "#f59e0b"];
const splitLine = { lineStyle: { color: GRID } };

/** The parts every chart shares. `title` is left out for a chart whose card already says what it is. */
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
  // The curves of a chip are told apart by their test point only when it has more than one.
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
      name: `${candidate.name} ${formatHz(candidate.freq)} Hz`,
      type: "line",
      yAxisIndex: 1,
      showSymbol: false,
      lineStyle: { type: "dashed" },
      data: zip(grid, candidate.vals),
    });
  }
  return {
    ...base(null, maxFreq),
    legend: { type: "scroll", top: 0, selected, textStyle: { color: text } },
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

/** The sum of the eligible chips' PSDs. It is for looking at; no recommendation comes from it. */
export function summedOption(summed: SummedPsd, maxFreq: number): EChartsCoreOption {
  return {
    ...base("Summed PSD (display only)", maxFreq),
    yAxis: { type: "value", name: "Frequency-normalised PSD", splitLine },
    series: [
      {
        name: "Summed PSD",
        type: "line",
        showSymbol: false,
        data: zip(summed.freq_bins, summed.psd),
      },
    ],
  };
}
