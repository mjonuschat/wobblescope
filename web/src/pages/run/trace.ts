import type { EChartsCoreOption } from "echarts/core";

import type { ChipLive } from "@/lib/generated/ChipLive";
import type { Direction } from "@/lib/generated/Direction";
import type { TestState } from "@/lib/generated/TestState";

export interface Trace {
  chip: string;
  points: [number, number][];
}

export interface Traces {
  key: string;
  series: Trace[];
}

export const NO_TRACES: Traces = { key: "", series: [] };

/** The signal along `axis`. A diagonal moves both X and Y, so its signal is the two combined. */
export function levelOf(chip: ChipLive, axis: Direction | null): number | null {
  if (!chip.level) return null;
  if (axis !== null && typeof axis === "object") return Math.hypot(chip.level[0], chip.level[1]);
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

export function liveSignalOption(series: Trace[], range: [number, number]): EChartsCoreOption {
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
      // The range is known before the sweep starts, so the axis does not rescale as points arrive.
      min: range[0],
      max: range[1],
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
