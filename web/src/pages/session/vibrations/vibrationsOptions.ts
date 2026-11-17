import type { EChartsCoreOption } from "echarts/core";

import type { MotorResult } from "@/lib/generated/MotorResult";
import type { VibrationsResult } from "@/lib/generated/VibrationsResult";
import type { Zone } from "@/lib/generated/Zone";

const TEXT = "#d4d4d4";
const MUTED = "#a3a3a3";
const GRID = "rgba(255, 255, 255, 0.08)";
const PALETTE = ["#ff7a33", "#4da3ff", "#7ee081", "#ffd166", "#c792ea", "#ef5b7b"];
const GOOD_BAND = "rgba(126, 224, 129, 0.18)";
const GOOD_LINE = "#7ee081";
const PEAK_LINE = "cyan";
const WARNING = "#ffd166";
/** An inferno-like ramp, dark to bright. */
const INFERNO = ["#000004", "#420a68", "#932667", "#dd513a", "#fca50a", "#fcffa4"];
const splitLine = { lineStyle: { color: GRID } };

const LOW_FREQ_WARNING =
  "Warning: a lot of low frequency vibrations were detected, the test was probably performed at too high an acceleration";

/**
 * The heatmaps draw every other angle row of the 720-row grid and every speed column. The full grid
 * is 720 x 6N cells (432,000 for 100 speeds), which ECharts draws too slowly. 360 rows keep one row
 * per degree.
 */
const ANGLE_STRIDE = 2;

const base = (): EChartsCoreOption => ({
  backgroundColor: "transparent",
  textStyle: { color: TEXT },
  color: PALETTE,
});

const round1 = (value: number) => Number(value.toFixed(1));

const mmPerSecond = (value: number) => `${round1(value)}`;

const degrees = (value: number) => `${round1(value)}`;

const cellText = (result: VibrationsResult, angles: number[], value: number[]) =>
  `${mmPerSecond(result.speeds[value[0]])} mm/s, ${degrees(angles[value[1]])} deg`;

/** The `interval` of a category axis that shows about `shown` of `count` labels. */
const labelInterval = (count: number, shown: number) => Math.max(0, Math.ceil(count / shown) - 1);

const clampIndex = (index: number, length: number) => Math.max(0, Math.min(index, length - 1));

/** The grid with every `ANGLE_STRIDE`th row, and the index of each kept row in the full grid. */
function downsample(result: VibrationsResult): { rows: number[][]; angles: number[] } {
  const rows: number[][] = [];
  const angles: number[] = [];
  for (let i = 0; i < result.grid.length; i += ANGLE_STRIDE) {
    rows.push(result.grid[i]);
    angles.push(result.angles[i]);
  }
  return { rows, angles };
}

/** Log10 of the cells, a cell at or below zero taking the log of the smallest positive cell. */
function logGrid(rows: number[][]): { logs: number[][]; min: number; max: number } {
  let smallest = Number.POSITIVE_INFINITY;
  for (const row of rows) for (const v of row) if (v > 0 && v < smallest) smallest = v;
  if (!Number.isFinite(smallest)) smallest = 1;
  const floor = Math.log10(smallest);
  let max = floor;
  const logs = rows.map((row) =>
    row.map((v) => {
      const log = v > 0 && Number.isFinite(v) ? Math.log10(v) : floor;
      if (log > max) max = log;
      return log;
    }),
  );
  return { logs, min: floor, max };
}

/** Log colour scale. Values are already log10, so the labels show the power of ten. */
function logVisualMap(min: number, max: number): Record<string, unknown> {
  return {
    type: "continuous",
    min,
    max: max > min ? max : min + 1,
    dimension: 2,
    calculable: false,
    orient: "vertical",
    right: 0,
    top: "middle",
    inRange: { color: INFERNO },
    textStyle: { color: TEXT },
    formatter: (value: number) => (10 ** value).toExponential(1),
  };
}

const zoneName = (n: number, result: VibrationsResult, zone: Zone) => {
  const last = result.speeds.length - 1;
  return `Zone ${n}: ${mmPerSecond(result.speeds[clampIndex(zone.start, last + 1)])} to ${mmPerSecond(result.speeds[clampIndex(zone.end, last + 1)])} mm/s`;
};

export function polarAngleOption(result: VibrationsResult): EChartsCoreOption {
  const radius = result.angle_energy.length > 0 ? Math.max(...result.angle_energy) : 1;
  const top = radius > 0 ? radius : 1;
  const last = result.angles.length;
  const sectors = result.good_angles.map((zone, n) => {
    const from = result.angles[clampIndex(zone.start, last)];
    const to = result.angles[clampIndex(zone.end, last)];
    const arc: number[][] = [[0, from]];
    const steps = 24;
    for (let i = 0; i <= steps; i += 1) arc.push([top, from + ((to - from) * i) / steps]);
    arc.push([0, to]);
    return {
      name: `Good angles ${n + 1}`,
      type: "line",
      coordinateSystem: "polar",
      showSymbol: false,
      silent: true,
      data: arc,
      lineStyle: { color: GOOD_LINE, width: 1, type: "dashed" },
      areaStyle: { color: GOOD_BAND },
    };
  });
  return {
    ...base(),
    title: {
      text: result.symmetry === null ? "Symmetry: not available" : `Symmetry: ${result.symmetry.toFixed(1)}%`,
      left: "center",
      textStyle: { color: TEXT, fontSize: 13, fontWeight: 500 },
    },
    tooltip: { trigger: "item" },
    polar: { radius: "70%", center: ["50%", "55%"] },
    angleAxis: {
      type: "value",
      min: 0,
      max: 360,
      startAngle: 0,
      clockwise: false,
      interval: 45,
      axisLabel: { color: MUTED, formatter: "{value} deg" },
      splitLine,
    },
    radiusAxis: { type: "value", min: 0, max: top, axisLabel: { color: MUTED }, splitLine },
    series: [
      {
        name: "Energy",
        type: "line",
        coordinateSystem: "polar",
        showSymbol: false,
        data: result.angle_energy.map((energy, i) => [energy, result.angles[i]]),
        lineStyle: { color: PALETTE[0] },
        areaStyle: { color: PALETTE[0], opacity: 0.3 },
      },
      ...sectors,
    ],
  };
}

export function polarHeatmapOption(result: VibrationsResult): EChartsCoreOption {
  const { rows, angles } = downsample(result);
  const { logs, min, max } = logGrid(rows);
  const data = logs.flatMap((row, a) => row.map((value, s) => [s, a, value]));
  return {
    ...base(),
    tooltip: {
      formatter: (p: { value: number[] }) => cellText(result, angles, p.value),
    },
    visualMap: logVisualMap(min, max),
    polar: { radius: "70%", center: ["45%", "52%"] },
    angleAxis: {
      type: "category",
      data: angles.map(degrees),
      startAngle: 0,
      clockwise: false,
      boundaryGap: false,
      axisLabel: { color: MUTED, interval: 44, formatter: "{value} deg" },
      splitLine: { show: false },
    },
    radiusAxis: {
      type: "category",
      name: "Speed (mm/s)",
      data: result.speeds.map(mmPerSecond),
      axisLabel: { color: MUTED, interval: labelInterval(result.speeds.length, 5) },
      splitLine: { show: false },
    },
    series: [{ name: "Energy", type: "heatmap", coordinateSystem: "polar", data, progressive: 0 }],
  };
}

/** Minimum, maximum and variance of the energy per speed, the vibration metric, peaks and good zones. */
export function speedProfileOption(result: VibrationsResult): EChartsCoreOption {
  const line = (values: number[]) => values.map((v, i) => [result.speeds[i], v]);
  const metricName = `Vibration metric (${result.peaks.length} bad peaks)`;
  return {
    ...base(),
    tooltip: { trigger: "axis" },
    legend: { top: 0, textStyle: { color: TEXT } },
    grid: { top: 48, left: 64, right: 64, bottom: 40 },
    xAxis: { type: "value", name: "Speed (mm/s)", min: "dataMin", max: "dataMax", splitLine },
    yAxis: [
      { type: "value", name: "Energy", splitLine },
      { type: "value", show: false, splitLine: { show: false } },
    ],
    series: [
      {
        name: "Minimum",
        type: "line",
        showSymbol: false,
        data: line(result.speed_min),
        markArea: {
          silent: true,
          itemStyle: { color: GOOD_BAND },
          label: { color: GOOD_LINE, position: "insideTop" },
          data: result.good_speeds.map((zone, n) => [
            { name: zoneName(n + 1, result, zone), xAxis: result.speeds[clampIndex(zone.start, result.speeds.length)] },
            { xAxis: result.speeds[clampIndex(zone.end, result.speeds.length)] },
          ]),
        },
      },
      { name: "Maximum", type: "line", showSymbol: false, data: line(result.speed_max) },
      {
        name: "Variance",
        type: "line",
        showSymbol: false,
        data: line(result.speed_variance),
        lineStyle: { type: "dashed" },
      },
      {
        name: metricName,
        type: "line",
        yAxisIndex: 1,
        showSymbol: false,
        data: line(result.vibration_metric),
        lineStyle: { color: PEAK_LINE },
        itemStyle: { color: PEAK_LINE },
        markPoint: {
          silent: true,
          symbol: "circle",
          symbolSize: 18,
          itemStyle: { color: PEAK_LINE },
          label: { color: "#171717", fontSize: 11 },
          data: result.peaks.map((peak, n) => ({
            name: `${n + 1}`,
            value: `${n + 1}`,
            coord: [result.speeds[peak], result.vibration_metric[peak]],
          })),
        },
      },
    ],
  };
}

/** The labels of the four plotted angles: X, Y and the diagonals, which are A and B on a CoreXY. */
function angleLabels(kinematics: string): [number, string][] {
  const corexy = kinematics === "corexy" || kinematics === "limited_corexy";
  return [
    [0, "X (0 deg)"],
    [90, "Y (90 deg)"],
    [45, corexy ? "A (45 deg)" : "45 deg"],
    [135, corexy ? "B (135 deg)" : "135 deg"],
  ];
}

/** The index of the first angle at or above `angle`, as numpy's searchsorted with side left. */
function nearestRow(angles: number[], angle: number): number {
  const found = angles.findIndex((a) => a >= angle);
  return clampIndex(found === -1 ? angles.length - 1 : found, angles.length);
}

/** Energy over speed at 0, 90, 45 and 135 degrees. */
export function angularProfilesOption(result: VibrationsResult, kinematics: string): EChartsCoreOption {
  return {
    ...base(),
    tooltip: { trigger: "axis" },
    legend: { top: 0, textStyle: { color: TEXT } },
    grid: { top: 48, left: 64, right: 64, bottom: 40 },
    xAxis: { type: "value", name: "Speed (mm/s)", min: "dataMin", max: "dataMax", splitLine },
    yAxis: { type: "value", name: "Energy", splitLine },
    series: angleLabels(kinematics).map(([angle, name]) => ({
      name,
      type: "line",
      showSymbol: false,
      data: (result.grid[nearestRow(result.angles, angle)] ?? []).map((v, i) => [result.speeds[i], v]),
    })),
  };
}

/** Speed on x and angle on y, with a cyan line at each bad peak. */
export function heatmapOption(result: VibrationsResult): EChartsCoreOption {
  const { rows, angles } = downsample(result);
  const { logs, min, max } = logGrid(rows);
  const data = logs.flatMap((row, a) => row.map((value, s) => [s, a, value]));
  return {
    ...base(),
    tooltip: {
      formatter: (p: { value: number[] }) => cellText(result, angles, p.value),
    },
    visualMap: logVisualMap(min, max),
    grid: { top: 32, left: 64, right: 96, bottom: 48 },
    xAxis: {
      type: "category",
      name: "Speed (mm/s)",
      data: result.speeds.map(mmPerSecond),
      axisLabel: { color: MUTED, interval: labelInterval(result.speeds.length, 10) },
      splitArea: { show: false },
    },
    yAxis: {
      type: "category",
      name: "Angle (deg)",
      data: angles.map(degrees),
      axisLabel: { color: MUTED, interval: 44 },
    },
    series: [
      {
        name: "Energy",
        type: "heatmap",
        data,
        progressive: 0,
        markLine: {
          silent: true,
          symbol: "none",
          data: result.peaks.map((peak, n) => ({
            xAxis: peak,
            lineStyle: { color: PEAK_LINE, type: "solid", width: 1 },
            label: {
              formatter: `Peak ${n + 1} (${mmPerSecond(result.peak_speeds[n] ?? result.speeds[peak])} mm/s)`,
              color: PEAK_LINE,
              position: "end",
            },
          })),
        },
      },
    ],
  };
}

export function motorProfileOption(motor: MotorResult, maxFreq: number, kinematics: string): EChartsCoreOption {
  const names = new Map(angleLabels(kinematics));
  const pair = (values: number[]) => values.map((v, i) => [motor.freqs[i], v]);
  const peak = motor.combined[motor.peak_index];
  const damping =
    motor.zeta === null ? "No damping ratio computed" : `Motor damping ratio (zeta): ${motor.zeta.toFixed(3)}`;
  const lines = [`Motor resonant frequency (w0): ${motor.fr.toFixed(1)} Hz`, damping];
  if (motor.low_freq_warning) lines.push(LOW_FREQ_WARNING);
  return {
    ...base(),
    tooltip: { trigger: "axis" },
    legend: { top: 0, textStyle: { color: TEXT } },
    title: {
      text: lines.join("\n"),
      left: 0,
      top: 24,
      textStyle: { color: motor.low_freq_warning ? WARNING : TEXT, fontSize: 12, fontWeight: 400 },
    },
    grid: { top: 40 + 18 * lines.length, left: 64, right: 64, bottom: 40 },
    xAxis: { type: "value", name: "Frequency (Hz)", min: 0, max: maxFreq, splitLine },
    yAxis: { type: "value", name: "Energy", splitLine },
    series: [
      {
        name: "Combined",
        type: "line",
        showSymbol: false,
        data: pair(motor.combined),
        lineStyle: { width: 2 },
        markPoint: {
          silent: true,
          symbol: "circle",
          symbolSize: 20,
          itemStyle: { color: PALETTE[0] },
          label: { color: "#171717", fontSize: 11 },
          data: [{ name: "R", value: "R", coord: [motor.freqs[motor.peak_index], peak] }],
        },
      },
      ...motor.profiles.map(([angle, values]) => ({
        name: names.get(angle) ?? `${angle} deg`,
        type: "line",
        showSymbol: false,
        data: pair(values),
        lineStyle: { type: "dashed", width: 1 },
      })),
    ],
  };
}
