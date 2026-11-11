import type { EChartsCoreOption } from "echarts/core";

import type { Axis } from "@/lib/generated/Axis";
import type { ChipLive } from "@/lib/generated/ChipLive";
import type { TestState } from "@/lib/generated/TestState";

/** One chip's signal against frequency for the segment being measured. */
export interface Trace {
  chip: string;
  /** `[Hz, signal]` pairs in the order they arrived. */
  points: [number, number][];
}

/** The traces of one segment. A new segment has a new key and starts empty. */
export interface Traces {
  key: string;
  series: Trace[];
}

export const NO_TRACES: Traces = { key: "", series: [] };

/** The chip's signal on the tested axis: X for an X segment, Y for a Y segment. */
export function levelOf(chip: ChipLive, axis: Axis | null): number | null {
  if (!chip.level) return null;
  return chip.level[axis === "Y" ? 1 : 0];
}

/**
 * Adds what `test` shows to `old`: for each chip, its signal at the frequency Klipper reported.
 * A frequency already on the trace keeps the larger signal, so the line has one point per Hz.
 * Returns `old` itself when nothing changed.
 */
export function extend(old: Traces, test: TestState): Traces {
  const key = `${test.id}:${test.segment}`;
  const base = old.key === key ? old : { key, series: [] };
  const hz = test.frequency_hz;
  if (hz === null) return base === old ? old : base;
  let changed = base !== old;
  const series = test.chips.map((chip) => {
    const before = base.series.find((s) => s.chip === chip.chip) ?? { chip: chip.chip, points: [] };
    const level = levelOf(chip, test.axis);
    if (level === null) return before;
    const last = before.points[before.points.length - 1];
    if (last && last[0] === hz) {
      if (level <= last[1]) return before;
      changed = true;
      return { ...before, points: [...before.points.slice(0, -1), [hz, level] as [number, number]] };
    }
    changed = true;
    return { ...before, points: [...before.points, [hz, level] as [number, number]] };
  });
  return changed ? { key, series } : old;
}

const text = "#d4d4d4";
const GRID = "rgba(255, 255, 255, 0.08)";
const COLORS = ["#ff5a1f", "#ffb347", "#e0431a", "#ffd699"];

/** A line per chip with Hz along the bottom and the signal up the side. */
export function liveSignalOption(series: Trace[]): EChartsCoreOption {
  return {
    backgroundColor: "transparent",
    textStyle: { color: text },
    animation: false,
    color: COLORS,
    tooltip: { trigger: "axis" },
    legend: series.length > 1 ? { top: 0, textStyle: { color: text } } : undefined,
    grid: { top: series.length > 1 ? 36 : 16, left: 64, right: 24, bottom: 36 },
    xAxis: {
      type: "value",
      name: "Hz",
      nameLocation: "middle",
      nameGap: 24,
      min: 0,
      splitLine: { lineStyle: { color: GRID } },
    },
    yAxis: { type: "value", name: "mm/s²", splitLine: { lineStyle: { color: GRID } } },
    series: series.map((s) => ({
      name: s.chip,
      type: "line",
      showSymbol: false,
      data: s.points,
    })),
  };
}
