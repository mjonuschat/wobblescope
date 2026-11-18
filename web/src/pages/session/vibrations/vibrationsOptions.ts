import type { EChartsCoreOption } from "echarts/core";

import type { MotorResult } from "@/lib/generated/MotorResult";
import type { VibrationsResult } from "@/lib/generated/VibrationsResult";
import { INFERNO, ON_HEATMAP } from "@/components/charts/colormaps";
import { DEFAULT_CHART_THEME, type ChartTheme } from "@/ui-kit/chartTheme";

const splitLine = (theme: ChartTheme) => ({ lineStyle: { color: theme.grid } });

/** Shown in the motor panel when Shake&Tune's low frequency check trips. */
export const LOW_FREQ_WARNING =
  "A lot of low frequency vibrations were detected, which can alter the readings. The test was probably run at too high an acceleration.";

/** The angle between the spokes and labels of the polar plots, in degrees. */
const POLAR_SPOKE = 15;

/** The distance between the speed labels of the heatmaps, in mm/s. */
const SPEED_TICK = 25;

const badge = (theme: ChartTheme, fill: string) => ({
  backgroundColor: fill,
  color: theme.onFill(fill),
  fontWeight: "bold",
  fontSize: 11,
  padding: [1, 4],
  borderRadius: 7,
});

const base = (theme: ChartTheme): EChartsCoreOption => ({
  backgroundColor: "transparent",
  textStyle: { color: theme.text },
  color: [theme.accent, ...theme.series],
});

export function siEnergy(value: number): string {
  if (!Number.isFinite(value) || value === 0) return "0";
  const units = ["", "k", "M", "G", "T"];
  const power = Math.max(0, Math.min(units.length - 1, Math.floor(Math.log10(Math.abs(value)) / 3)));
  const scaled = value / 1000 ** power;
  return `${Number(scaled.toPrecision(3))}${units[power]}`;
}

const clampIndex = (index: number, length: number) => Math.max(0, Math.min(index, length - 1));

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

const INFERNO_STOPS = INFERNO.map((hex) => [1, 3, 5].map((k) => Number.parseInt(hex.slice(k, k + 2), 16)));

function inferno(t: number): [number, number, number] {
  const stops = INFERNO_STOPS;
  const at = Math.max(0, Math.min(1, t)) * (stops.length - 1);
  const low = Math.min(Math.floor(at), stops.length - 2);
  const mix = at - low;
  return [0, 1, 2].map((c) => Math.round(stops[low][c] * (1 - mix) + stops[low + 1][c] * mix)) as [
    number,
    number,
    number,
  ];
}

/**
 * A bitmap of `width` x `height` pixels, each coloured by `value(x, y)` on the inferno ramp between
 * `min` and `max`. A pixel with no value stays transparent. One bitmap replaces the hundreds of
 * thousands of rectangles that a heatmap of the whole grid would need.
 */
function raster(
  width: number,
  height: number,
  min: number,
  max: number,
  value: (x: number, y: number) => number | null,
): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) return canvas;
  const image = context.createImageData(width, height);
  const span = max > min ? max - min : 1;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const v = value(x, y);
      if (v === null) continue;
      const [r, g, b] = inferno((v - min) / span);
      const at = (y * width + x) * 4;
      image.data[at] = r;
      image.data[at + 1] = g;
      image.data[at + 2] = b;
      image.data[at + 3] = 255;
    }
  }
  context.putImageData(image, 0, 0);
  return canvas;
}

/** The two extreme cells, which give the colour bar its range. The bitmap is drawn with the first. */
const extremes = (xMin: number, yMin: number, min: number, max: number) => [
  [xMin, yMin, min],
  [xMin, yMin, max],
];

/** Log colour scale. Values are already log10, so the labels show the power of ten. */
function logVisualMap(min: number, max: number, theme: ChartTheme): Record<string, unknown> {
  const top = max > min ? max : min + 1;
  return {
    type: "continuous",
    min,
    max: top,
    dimension: 2,
    calculable: false,
    orient: "vertical",
    right: 8,
    top: "middle",
    itemHeight: 180,
    inRange: { color: INFERNO },
    textStyle: { color: theme.text },
    text: [siEnergy(10 ** top), siEnergy(10 ** min)],
  };
}

/** Where the radial scale of the polar plots runs, as in Shake&Tune: clear of the 0 degree label. */
const SCALE_ANGLE = 25;

/** A step of 1, 2 or 5 times a power of ten that puts about `divisions` rings in `max`. */
export function niceStep(max: number, divisions: number): number {
  if (!(max > 0)) return 1;
  const raw = max / divisions;
  const power = 10 ** Math.floor(Math.log10(raw));
  const fraction = raw / power;
  return (fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 5 ? 5 : 10) * power;
}

const ringValues = (max: number, step: number) =>
  Array.from({ length: Math.floor(max / step + 1e-9) }, (_, i) => (i + 1) * step);

/**
 * The radial scale of a polar plot, drawn as a faint axis at SCALE_ANGLE with a tick and a label at
 * each value, the labels alternating between the two sides of the axis. ECharts puts a radius axis
 * on the angle axis's start, where it runs into the 0 degree label and the plot's first spoke. With
 * `grid`, it also draws a ring at each value and a spoke every `grid.spokes` degrees, above a bitmap
 * that would hide the axis's own.
 */
function polarScale(
  values: number[],
  max: number,
  label: (value: number) => string,
  text: string,
  border: string,
  grid?: { color: string; spokes: number },
) {
  return {
    name: "Scale",
    type: "custom",
    coordinateSystem: "polar",
    silent: true,
    z: 6,
    data: [[max, SCALE_ANGLE]],
    encode: { radius: 0, angle: 1 },
    renderItem: (_params: unknown, api: { coord: (value: number[]) => number[] }) => {
      const [x0, y0] = api.coord([0, SCALE_ANGLE]);
      const [x1, y1] = api.coord([max, SCALE_ANGLE]);
      const length = Math.hypot(x1 - x0, y1 - y0) || 1;
      // A unit step to the side of the axis, towards its counter-clockwise side.
      const sideX = (y1 - y0) / length;
      const sideY = -(x1 - x0) / length;
      const children: Record<string, unknown>[] = [];
      if (grid) {
        for (const value of values) {
          const [x, y] = api.coord([value, SCALE_ANGLE]);
          const shape = { cx: x0, cy: y0, r: Math.hypot(x - x0, y - y0) };
          children.push({ type: "circle", shape, style: { stroke: grid.color, lineWidth: 1, fill: "none" } });
        }
        for (let angle = 0; angle < 360; angle += grid.spokes) {
          const [x, y] = api.coord([max, angle]);
          children.push({
            type: "line",
            shape: { x1: x0, y1: y0, x2: x, y2: y },
            style: { stroke: grid.color, lineWidth: 1 },
          });
        }
      }
      children.push({
        type: "line",
        shape: { x1: x0, y1: y0, x2: x1, y2: y1 },
        style: { stroke: text, lineWidth: 1, opacity: 0.6 },
      });
      for (const [index, value] of values.entries()) {
        const [x, y] = api.coord([value, SCALE_ANGLE]);
        // The labels take turns on either side of the axis, so neighbours do not touch.
        const side = index % 2 === 0 ? 1 : -1;
        children.push({
          type: "line",
          shape: { x1: x - sideX * 4, y1: y - sideY * 4, x2: x + sideX * 4, y2: y + sideY * 4 },
          style: { stroke: text, lineWidth: 1 },
        });
        children.push({
          type: "text",
          style: {
            text: label(value),
            x: x + side * sideX * 12,
            y: y + side * sideY * 12,
            fill: text,
            stroke: border,
            lineWidth: 2,
            fontSize: 10,
            textAlign: "center",
            textVerticalAlign: "middle",
          },
        });
      }
      return { type: "group", children };
    },
  };
}

export function polarAngleOption(result: VibrationsResult, theme: ChartTheme = DEFAULT_CHART_THEME): EChartsCoreOption {
  const radius = result.angle_energy.length > 0 ? Math.max(...result.angle_energy) : 1;
  const top = radius > 0 ? radius : 1;
  // Shake&Tune keeps the rim above the highest energy, so a good range reads even at the peak.
  const rim = top * 1.05;
  const energyStep = niceStep(rim, 8);
  const last = result.angles.length;
  /** The good ranges, as in Shake&Tune: the band between the energy curve and the rim, with a dotted line at each edge. */
  const goodAngles = {
    name: "Good angles",
    type: "custom",
    coordinateSystem: "polar",
    silent: true,
    data: result.good_angles.map((zone) => [rim, result.angles[clampIndex(zone.start, last)]]),
    encode: { radius: 0, angle: 1 },
    renderItem: (params: { dataIndex: number }, api: { coord: (value: number[]) => number[] }) => {
      const zone = result.good_angles[params.dataIndex];
      const start = clampIndex(zone.start, last);
      const end = clampIndex(zone.end, last);
      const point = (r: number, angle: number) => {
        const [x, y] = api.coord([r, angle]);
        return [x, y];
      };
      const outer: number[][] = [];
      const inner: number[][] = [];
      for (let i = start; i <= end; i += 1) outer.push(point(rim, result.angles[i]));
      for (let i = end; i >= start; i -= 1) inner.push(point(result.angle_energy[i], result.angles[i]));
      const edge = (i: number) => ({
        type: "polyline",
        shape: { points: [point(result.angle_energy[i], result.angles[i]), point(rim, result.angles[i])] },
        style: { stroke: theme.danger, lineWidth: 1.5, lineDash: [2, 3], fill: "none" },
      });
      // The badge of the summary table row, in the middle of the band.
      const middle = Math.round((start + end) / 2);
      const [x, y] = point((result.angle_energy[middle] + rim) / 2, result.angles[middle]);
      return {
        type: "group",
        children: [
          {
            type: "polygon",
            shape: { points: [...outer, ...inner] },
            style: { fill: theme.alpha(theme.success, 0.18) },
          },
          edge(start),
          edge(end),
          { type: "circle", shape: { cx: x, cy: y, r: 8 }, style: { fill: theme.success } },
          {
            type: "text",
            style: {
              text: `${params.dataIndex + 1}`,
              x,
              y,
              fill: theme.onFill(theme.success),
              fontSize: 10,
              fontWeight: "bold",
              textAlign: "center",
              textVerticalAlign: "middle",
            },
          },
        ],
      };
    },
  };
  return {
    ...base(theme),
    title: {
      text: result.symmetry === null ? "Symmetry: not available" : `Symmetry: ${result.symmetry.toFixed(1)}%`,
      left: "center",
      textStyle: { color: theme.text, fontSize: 13, fontWeight: 500 },
    },
    tooltip: { trigger: "item" },
    polar: { radius: "74%", center: ["50%", "55%"] },
    angleAxis: {
      type: "value",
      min: 0,
      max: 360,
      startAngle: 0,
      clockwise: false,
      interval: POLAR_SPOKE,
      axisLabel: { color: theme.text, fontSize: 12, formatter: "{value}°" },
      splitLine: { show: false },
    },
    // The rings and spokes are drawn with the scale, above the filled area (see polarScale).
    radiusAxis: {
      type: "value",
      min: 0,
      max: rim,
      interval: energyStep,
      axisLabel: { show: false },
      axisTick: { show: false },
      axisLine: { show: false },
      splitLine: { show: false },
    },
    series: [
      goodAngles,
      {
        name: "Energy",
        type: "line",
        coordinateSystem: "polar",
        showSymbol: false,
        data: result.angle_energy.map((energy, i) => [energy, result.angles[i]]),
        lineStyle: { color: theme.accent },
        areaStyle: { color: theme.accent, opacity: 0.3 },
      },
      polarScale(ringValues(rim, energyStep), rim, siEnergy, theme.muted, theme.surface, {
        color: ON_HEATMAP.gridSoft,
        spokes: POLAR_SPOKE,
      }),
    ],
  };
}

/**
 * Speed as the radius and angle around. ECharts' heatmap series cannot sit on polar axes, so a
 * custom series places one bitmap over the polar plot, each pixel taking the value of the grid cell
 * at its radius and angle.
 */
export function polarHeatmapOption(
  result: VibrationsResult,
  theme: ChartTheme = DEFAULT_CHART_THEME,
): EChartsCoreOption {
  const { logs, min, max } = logGrid(result.grid);
  const speeds = result.speeds;
  const speedMin = speeds[0] ?? 0;
  const speedMax = speeds[speeds.length - 1] ?? 1;
  const speedStep = speeds.length > 1 ? (speedMax - speedMin) / (speeds.length - 1) : 1;
  const rows = logs.length;
  const columns = speeds.length;
  let bitmap: HTMLCanvasElement | null = null;
  const draw = () => {
    if (bitmap) return bitmap;
    const size = 720;
    const half = size / 2;
    bitmap = raster(size, size, min, max, (x, y) => {
      const dx = x + 0.5 - half;
      const dy = half - (y + 0.5);
      const radius = Math.hypot(dx, dy) / half;
      if (radius > 1) return null;
      const turn = (Math.atan2(dy, dx) + 2 * Math.PI) % (2 * Math.PI);
      const row = Math.min(rows - 1, Math.round((turn / (2 * Math.PI)) * (rows - 1)));
      // The radius axis starts at 0 mm/s, below the slowest measured speed, which the centre takes.
      const column = Math.max(0, Math.min(columns - 1, Math.round((radius * speedMax - speedMin) / speedStep)));
      return logs[row][column];
    });
    return bitmap;
  };
  return {
    ...base(theme),
    tooltip: { show: false },
    // The radius axis name would collide with the 0 degree label, so the unit goes below the plot.
    title: {
      text: "Radius: speed (mm/s)",
      left: "42%",
      bottom: 4,
      textStyle: { color: theme.muted, fontSize: 12, fontWeight: 400 },
    },
    visualMap: logVisualMap(min, max, theme),
    polar: { radius: "74%", center: ["45%", "50%"] },
    angleAxis: {
      type: "value",
      min: 0,
      max: 360,
      startAngle: 0,
      clockwise: false,
      interval: POLAR_SPOKE,
      axisLabel: { color: theme.text, fontSize: 12, formatter: "{value}°" },
      splitLine: { show: false },
    },
    // The scale is drawn on a diagonal, every 25 mm/s from the centre, above the bitmap (see polarScale).
    radiusAxis: {
      type: "value",
      min: 0,
      max: speedMax,
      interval: SPEED_TICK,
      axisLabel: { show: false },
      axisTick: { show: false },
      axisLine: { show: false },
      splitLine: { show: false },
    },
    series: [
      {
        name: "Energy",
        type: "custom",
        coordinateSystem: "polar",
        data: extremes(speedMin, 0, min, max),
        encode: { radius: 0, angle: 1 },
        silent: true,
        renderItem: (params: { dataIndex: number; coordSys: { cx: number; cy: number; r: number } }) => {
          if (params.dataIndex !== 0) return null;
          const { cx, cy, r } = params.coordSys;
          return { type: "image", style: { image: draw(), x: cx - r, y: cy - r, width: 2 * r, height: 2 * r } };
        },
      },
      polarScale(
        ringValues(speedMax, SPEED_TICK),
        speedMax,
        (value) => `${value}`,
        ON_HEATMAP.text,
        ON_HEATMAP.outline,
        {
          color: ON_HEATMAP.gridStrong,
          spokes: POLAR_SPOKE,
        },
      ),
    ],
  };
}

export function speedProfileOption(
  result: VibrationsResult,
  theme: ChartTheme = DEFAULT_CHART_THEME,
): EChartsCoreOption {
  const line = (values: number[]) => values.map((v, i) => [result.speeds[i], v]);
  const metricName = `Vibration metric (${result.peaks.length} bad peaks)`;
  return {
    ...base(theme),
    // The vibration metric is what the chart is about, so it takes the accent and the rest stay neutral.
    color: [theme.neutral[2], theme.neutral[0], theme.neutral[1], theme.accent],
    tooltip: { trigger: "axis" },
    legend: { type: "scroll", top: 0, textStyle: { color: theme.text } },
    grid: { top: 48, left: 64, right: 64, bottom: 40 },
    xAxis: { type: "value", name: "Speed (mm/s)", min: "dataMin", max: "dataMax", splitLine: splitLine(theme) },
    yAxis: [
      { type: "value", name: "Energy", splitLine: splitLine(theme) },
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
          itemStyle: { color: theme.alpha(theme.success, 0.18) },
          label: { position: "insideTop", rich: { zone: badge(theme, theme.success) } },
          data: result.good_speeds.map((zone, n) => [
            { name: `{zone|${n + 1}}`, xAxis: result.speeds[clampIndex(zone.start, result.speeds.length)] },
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
        lineStyle: { color: theme.accent },
        itemStyle: { color: theme.accent },
        markPoint: {
          silent: true,
          symbol: "circle",
          symbolSize: 6,
          itemStyle: { color: theme.text },
          data: result.peaks.map((peak, n) => ({
            name: `${n + 1}`,
            coord: [result.speeds[peak], result.vibration_metric[peak]],
            label: {
              formatter: `{index|${n + 1}}`,
              position: "top",
              offset: [0, -2],
              rich: { index: badge(theme, theme.accent) },
            },
          })),
        },
      },
    ],
  };
}

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

export function angularProfilesOption(
  result: VibrationsResult,
  kinematics: string,
  theme: ChartTheme = DEFAULT_CHART_THEME,
): EChartsCoreOption {
  return {
    ...base(theme),
    // The four angles carry no lead, so they take the categorical colors.
    color: [...theme.series],
    tooltip: { trigger: "axis" },
    legend: { type: "scroll", top: 0, textStyle: { color: theme.text } },
    grid: { top: 48, left: 64, right: 64, bottom: 40 },
    xAxis: { type: "value", name: "Speed (mm/s)", min: "dataMin", max: "dataMax", splitLine: splitLine(theme) },
    yAxis: { type: "value", name: "Energy", splitLine: splitLine(theme) },
    series: angleLabels(kinematics).map(([angle, name]) => ({
      name,
      type: "line",
      showSymbol: false,
      data: (result.grid[nearestRow(result.angles, angle)] ?? []).map((v, i) => [result.speeds[i], v]),
    })),
  };
}

export function heatmapOption(result: VibrationsResult, theme: ChartTheme = DEFAULT_CHART_THEME): EChartsCoreOption {
  const { logs, min, max } = logGrid(result.grid);
  const speeds = result.speeds;
  const speedMin = speeds[0] ?? 0;
  const speedMax = speeds[speeds.length - 1] ?? 1;
  const rows = logs.length;
  let bitmap: HTMLCanvasElement | null = null;
  const draw = () => {
    // Angle 0 is the bottom row of the plot, which is the last row of the bitmap.
    bitmap ??= raster(speeds.length, rows, min, max, (x, y) => logs[rows - 1 - y][x]);
    return bitmap;
  };
  return {
    ...base(theme),
    // No content: the cross hair and its axis labels read the speed and angle under the pointer.
    tooltip: { trigger: "axis", showContent: false, axisPointer: { type: "cross", snap: false } },
    visualMap: logVisualMap(min, max, theme),
    grid: { top: 32, left: 64, right: 80, bottom: 48 },
    xAxis: {
      type: "value",
      name: "Speed (mm/s)",
      min: 0,
      max: speedMax,
      interval: SPEED_TICK,
      splitLine: { show: false },
    },
    yAxis: { type: "value", name: "Angle (deg)", min: 0, max: 360, interval: 45, splitLine: { show: false } },
    series: [
      {
        name: "Energy",
        type: "custom",
        data: extremes(speedMin, 0, min, max),
        encode: { x: 0, y: 1 },
        silent: true,
        renderItem: (params: { dataIndex: number }, api: { coord: (value: number[]) => number[] }) => {
          if (params.dataIndex !== 0) return null;
          // The speed axis starts at 0, so the bitmap covers only the measured speeds.
          const [left, top] = api.coord([speedMin, 360]);
          const [right, bottom] = api.coord([speedMax, 0]);
          return {
            type: "image",
            style: { image: draw(), x: left, y: top, width: right - left, height: bottom - top },
          };
        },
      },
      {
        name: "Bad peaks",
        type: "line",
        data: [],
        markLine: {
          silent: true,
          symbol: "none",
          data: result.peaks.map((peak, n) => ({
            xAxis: result.peak_speeds[n] ?? speeds[peak],
            lineStyle: { color: theme.accent, type: "solid", width: 1 },
            // Along the line, as Shake&Tune labels it, so ten of them do not overlap.
            label: {
              formatter: `Peak ${n + 1} (${Number((result.peak_speeds[n] ?? speeds[peak]).toFixed(1))} mm/s)`,
              position: "insideEndTop",
              color: theme.accent,
              fontSize: 11,
              textBorderColor: ON_HEATMAP.outline,
              textBorderWidth: 2,
            },
          })),
        },
      },
    ],
  };
}

export function motorProfileOption(
  motor: MotorResult,
  maxFreq: number,
  kinematics: string,
  theme: ChartTheme = DEFAULT_CHART_THEME,
): EChartsCoreOption {
  const names = new Map(angleLabels(kinematics));
  const pair = (values: number[]) => values.map((v, i) => [motor.freqs[i], v]);
  const peak = motor.combined[motor.peak_index];
  return {
    ...base(theme),
    tooltip: { trigger: "axis" },
    legend: { type: "scroll", top: 0, textStyle: { color: theme.text } },
    grid: { top: 48, left: 64, right: 64, bottom: 40 },
    xAxis: { type: "value", name: "Frequency (Hz)", min: 0, max: maxFreq, splitLine: splitLine(theme) },
    yAxis: { type: "value", name: "Energy", splitLine: splitLine(theme) },
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
          symbolSize: 6,
          itemStyle: { color: theme.text },
          data: [
            {
              name: "R",
              coord: [motor.freqs[motor.peak_index], peak],
              label: {
                formatter: "{index|R}",
                position: "top",
                offset: [0, -2],
                rich: { index: badge(theme, theme.muted) },
              },
            },
          ],
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
