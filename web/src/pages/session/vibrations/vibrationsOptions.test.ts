import * as echarts from "echarts/core";
import { SVGRenderer } from "echarts/renderers";
import { afterEach, describe, expect, it, vi } from "vitest";

import { CHART_PARTS } from "@/components/charts/register";
import { ON_HEATMAP } from "@/components/charts/colormaps";
import { DEFAULT_CHART_THEME } from "@/ui-kit/chartTheme";
import { linspaceVibrationsResult, motorResult, vibrationsResult } from "@/test/vibrations";

import {
  LOW_FREQ_WARNING,
  niceStep,
  angularProfilesOption,
  heatmapOption,
  motorProfileOption,
  polarAngleOption,
  polarHeatmapOption,
  siEnergy,
  speedProfileOption,
} from "./vibrationsOptions";

const GOOD_COLOR = DEFAULT_CHART_THEME.success;
const PEAK_COLOR = DEFAULT_CHART_THEME.accent;

// biome-ignore lint/suspicious/noExplicitAny: option objects are inspected loosely
type Loose = Record<string, any>;

const series = (option: unknown): Loose[] => (option as Loose).series;
const energy = (option: unknown): Loose => series(option)[1];

/** The shapes the radial scale of a polar plot draws, with a polar point mapped to pixels as angle counts counter-clockwise. */
const scale = (option: Loose): Loose[] => {
  const last = series(option)[series(option).length - 1];
  expect(last.name).toBe("Scale");
  const api = {
    coord: ([r, angle]: number[]) => [r * Math.cos((angle * Math.PI) / 180), -r * Math.sin((angle * Math.PI) / 180)],
  };
  return last.renderItem({ dataIndex: 0 }, api).children;
};

/** The shapes the good angles series draws for zone `n`, with polar points kept as [radius, angle]. */
const good = (option: Loose, n: number): Loose =>
  series(option)[0].renderItem({ dataIndex: n }, { coord: ([r, angle]: number[]) => [r, angle] });

interface Bitmap {
  width: number;
  height: number;
  data: Uint8ClampedArray;
}

/** A canvas context that keeps the pixels written to it and measures text, as jsdom has no canvas. */
function stubCanvas(): Bitmap[] {
  const written: Bitmap[] = [];
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation((() => ({
    createImageData: (width: number, height: number) => ({
      width,
      height,
      data: new Uint8ClampedArray(width * height * 4),
    }),
    putImageData: (image: Bitmap) => written.push(image),
    // zrender measures text with a canvas when it lays out titles and labels.
    measureText: (text: string) => ({ width: text.length * 6 }),
  })) as never);
  // The SVG renderer turns a bitmap into a data URL.
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:,");
  return written;
}

const pixel = (bitmap: Bitmap, x: number, y: number) => [
  ...bitmap.data.slice((y * bitmap.width + x) * 4, (y * bitmap.width + x) * 4 + 4),
];

afterEach(() => vi.restoreAllMocks());

function assertFinite(value: unknown, path = "option"): void {
  if (typeof value === "number") {
    expect(Number.isFinite(value), path).toBe(true);
  } else if (Array.isArray(value)) {
    for (const [i, v] of value.entries()) assertFinite(v, `${path}[${i}]`);
  } else if (value && typeof value === "object") {
    for (const [k, v] of Object.entries(value)) assertFinite(v, `${path}.${k}`);
  }
}

const allOptions = () => {
  const r = vibrationsResult();
  return [
    polarAngleOption(r),
    polarHeatmapOption(r),
    speedProfileOption(r),
    angularProfilesOption(r, "corexy"),
    heatmapOption(r),
    motorProfileOption(r.motor!, 100, "corexy"),
  ];
};

describe("polarAngleOption", () => {
  it("draws a filled polar line over a counter-clockwise angle axis from east", () => {
    const option = polarAngleOption(vibrationsResult()) as Loose;
    expect(option.polar).toBeDefined();
    expect(option.angleAxis).toMatchObject({ startAngle: 0, clockwise: false, min: 0, max: 360 });
    const main = energy(option);
    expect(main.type).toBe("line");
    expect(main.coordinateSystem).toBe("polar");
    expect(main.areaStyle).toBeDefined();
    expect(main.data).toHaveLength(9);
  });

  it("shows the symmetry, or that it is not available", () => {
    expect(JSON.stringify(polarAngleOption(vibrationsResult()))).toContain("Symmetry: 87.7%");
    expect(JSON.stringify(polarAngleOption(vibrationsResult({ symmetry: null })))).toContain("Symmetry: not available");
  });

  it("keeps the rim 5% above the highest energy, as Shake&Tune does, and the energy line over the good ranges", () => {
    const option = polarAngleOption(vibrationsResult()) as Loose;
    expect(option.radiusAxis.max).toBeCloseTo(3 * 1.05, 9);
    expect(series(option).map((s) => s.name)).toEqual(["Good angles", "Energy", "Scale"]);
  });

  it("draws the ring labels on a diagonal scale, 1G apart here, and none on the native axis", () => {
    const r = vibrationsResult({ angle_energy: [1e9, 6.3e9, 3e9, 2e9, 1e9, 2e9, 3e9, 2e9, 1e9] });
    const option = polarAngleOption(r) as Loose;
    expect(option.radiusAxis.axisLabel.show).toBe(false);
    expect(option.radiusAxis.axisLine.show).toBe(false);
    expect(option.radiusAxis.interval).toBe(1e9);
    const labels = scale(option).filter((c) => c.type === "text");
    expect(labels.map((c) => c.style.text)).toEqual(["1G", "2G", "3G", "4G", "5G", "6G"]);
    for (const label of labels) expect(label.style.text.length).toBeLessThanOrEqual(5);
  });

  it("draws the grid as Shake&Tune does: a pale ring per label and a spoke and label every 15 degrees", () => {
    const r = vibrationsResult({ angle_energy: [1e9, 6.3e9, 3e9, 2e9, 1e9, 2e9, 3e9, 2e9, 1e9] });
    const option = polarAngleOption(r) as Loose;
    expect(option.angleAxis.interval).toBe(15);
    expect(option.angleAxis.axisLabel.formatter).toBe("{value}°");
    // The grid is drawn above the filled area, so the native lines are off.
    expect(option.angleAxis.splitLine.show).toBe(false);
    expect(option.radiusAxis.splitLine.show).toBe(false);
    const grid = "rgba(176, 186, 198, 0.45)";
    const children = scale(option);
    expect(children.filter((c) => c.type === "circle")).toHaveLength(6);
    expect(children.filter((c) => c.type === "circle").every((c) => c.style.stroke === grid)).toBe(true);
    expect(children.filter((c) => c.type === "line" && c.style.stroke === grid)).toHaveLength(24);
  });

  it("picks a step of 1, 2 or 5 times a power of ten that gives about eight rings", () => {
    expect(niceStep(6.615e9, 8)).toBe(1e9);
    expect(niceStep(3.15, 8)).toBe(0.5);
    expect(niceStep(100, 8)).toBe(20);
    expect(niceStep(0, 8)).toBe(1);
    expect(niceStep(Number.NaN, 8)).toBe(1);
  });

  it("alternates the labels between the two sides of the scale and ticks each ring", () => {
    const children = scale(
      polarAngleOption(vibrationsResult({ angle_energy: [1e9, 6.3e9, 3e9, 1e9, 1e9, 1e9, 1e9, 1e9, 1e9] })) as Loose,
    );
    // The axis is the faint line, and a tick is a line in the scale's own colour; the rest are the grid.
    const axis = children.find((c) => c.type === "line" && c.style.opacity === 0.6) as Loose;
    const ticks = children.filter(
      (c) => c.type === "line" && c.style.stroke === DEFAULT_CHART_THEME.muted && c.style.opacity === undefined,
    );
    const labels = children.filter((c) => c.type === "text");
    expect(ticks).toHaveLength(labels.length);
    const direction = [axis.shape.x2 - axis.shape.x1, axis.shape.y2 - axis.shape.y1];
    // Which side of the axis a point is on: the sign of its cross product with the axis direction.
    const sideOf = (x: number, y: number, from: number[]) =>
      Math.sign(direction[0] * (y - from[1]) - direction[1] * (x - from[0]));
    const sides = labels.map((label, i) => {
      const tick = ticks[i].shape;
      return sideOf(label.style.x, label.style.y, [(tick.x1 + tick.x2) / 2, (tick.y1 + tick.y2) / 2]);
    });
    expect(sides.every((s) => s !== 0)).toBe(true);
    for (let i = 1; i < sides.length; i += 1) expect(sides[i], `label ${i + 1}`).toBe(-sides[i - 1]);
  });

  it("has one good range per zone", () => {
    expect(series(polarAngleOption(vibrationsResult()))[0].data).toHaveLength(1);
    expect(series(polarAngleOption(vibrationsResult({ good_angles: [] })))[0].data).toHaveLength(0);
  });

  it("shades the band between the energy curve and the rim, with a red dotted line at each edge", () => {
    const r = vibrationsResult();
    const option = polarAngleOption(r) as Loose;
    const group = good(option, 0);
    const [fill, first, last] = group.children;
    const rim = option.radiusAxis.max;
    expect(fill.type).toBe("polygon");
    // The zone covers indices 1 to 3 (45 to 135 degrees). The outer edge runs along the rim, the
    // inner one back along the energy curve.
    expect(fill.shape.points).toEqual([
      [rim, 45],
      [rim, 90],
      [rim, 135],
      [r.angle_energy[3], 135],
      [r.angle_energy[2], 90],
      [r.angle_energy[1], 45],
    ]);
    for (const [edge, index] of [
      [first, 1],
      [last, 3],
    ] as const) {
      expect(edge.type).toBe("polyline");
      expect(edge.shape.points).toEqual([
        [r.angle_energy[index], r.angles[index]],
        [rim, r.angles[index]],
      ]);
      expect(edge.style.lineDash).toBeDefined();
      expect(edge.style.stroke).toBe(DEFAULT_CHART_THEME.danger);
    }
  });

  it("numbers each band as its row in the summary table, in the middle of the band", () => {
    const r = vibrationsResult({
      good_angles: [
        { start: 1, end: 3, energy: 20 },
        { start: 5, end: 7, energy: 30 },
      ],
    });
    const option = polarAngleOption(r) as Loose;
    const rim = option.radiusAxis.max;
    for (const [n, middle] of [
      [0, 2],
      [1, 6],
    ] as const) {
      const [, , , badge, label] = good(option, n).children;
      expect(badge.type).toBe("circle");
      expect(badge.style.fill).toBe(GOOD_COLOR);
      // The badge sits halfway between the energy curve and the rim, at the middle angle.
      expect([badge.shape.cx, badge.shape.cy]).toEqual([(r.angle_energy[middle] + rim) / 2, r.angles[middle]]);
      expect(label.style.text).toBe(`${n + 1}`);
      expect([label.style.x, label.style.y]).toEqual([badge.shape.cx, badge.shape.cy]);
    }
  });

  it("spans angles[start] to angles[end] on linspace angles", () => {
    const r = linspaceVibrationsResult({ good_angles: [{ start: 100, end: 300, energy: 1 }] });
    const option = polarAngleOption(r) as Loose;
    const [fill] = good(option, 0).children;
    expect(fill.shape.points[0][1]).toBeCloseTo(r.angles[100], 9);
    expect(fill.shape.points[200][1]).toBeCloseTo(r.angles[300], 9);
    expect(fill.shape.points).toHaveLength(2 * 201);
  });

  it("keeps a zone that ends past the grid on the grid", () => {
    const r = vibrationsResult({ good_angles: [{ start: 7, end: 9, energy: 1 }] });
    const [fill] = good(polarAngleOption(r) as Loose, 0).children;
    expect(fill.shape.points.every((p: number[]) => Number.isFinite(p[0]) && Number.isFinite(p[1]))).toBe(true);
    expect(fill.shape.points).toHaveLength(2 * 2);
  });
});

describe("polarHeatmapOption", () => {
  it("is one bitmap on polar axes with speed as radius and angle around, on a log scale", () => {
    const r = vibrationsResult();
    const option = polarHeatmapOption(r) as Loose;
    expect(option.polar).toBeDefined();
    expect(option.angleAxis).toMatchObject({ type: "value", min: 0, max: 360, clockwise: false });
    // From 0 mm/s in 25 mm/s steps. The labels are on the diagonal scale, which is drawn above the bitmap.
    expect(option.radiusAxis).toMatchObject({ type: "value", min: 0, max: 80, interval: 25 });
    expect(option.radiusAxis.axisLabel.show).toBe(false);
    expect(series(option)[0]).toMatchObject({ type: "custom", coordinateSystem: "polar" });
    expect(JSON.stringify(option.title)).toContain("Radius: speed (mm/s)");
  });

  it("labels the speeds every 25 mm/s on a diagonal scale above the bitmap", () => {
    const option = polarHeatmapOption(vibrationsResult({ speeds: [0, 100, 200, 300] })) as Loose;
    expect(series(option).map((s) => s.name)).toEqual(["Energy", "Scale"]);
    const labels = scale(option).filter((c) => c.type === "text");
    expect(labels.map((c) => c.style.text)).toEqual([
      "25",
      "50",
      "75",
      "100",
      "125",
      "150",
      "175",
      "200",
      "225",
      "250",
      "275",
      "300",
    ]);
    // A ring at every label, growing outwards, drawn above the bitmap that would hide the axis's own.
    const rings = scale(option).filter((c) => c.type === "circle");
    expect(rings).toHaveLength(12);
    const radii = rings.map((c) => c.shape.r);
    expect(radii).toEqual([...radii].sort((a, b) => a - b));
    expect(radii[0]).toBeCloseTo(25, 6);
    expect(radii[11]).toBeCloseTo(300, 6);
    expect(rings[0].style).toMatchObject({ fill: "none", lineWidth: 1, stroke: ON_HEATMAP.gridStrong });
    // White on black, as the bitmap is bright and dark by turns.
    expect(labels[0].style).toMatchObject({ fill: ON_HEATMAP.text, stroke: ON_HEATMAP.outline });
  });

  it("draws the grid as Shake&Tune does: a spoke and a label every 15 degrees, in pale lines", () => {
    const option = polarHeatmapOption(vibrationsResult()) as Loose;
    expect(option.angleAxis.interval).toBe(15);
    expect(option.angleAxis.axisLabel.formatter).toBe("{value}°");
    const grid = "rgba(176, 186, 198, 0.9)";
    const spokes = scale(option).filter((c) => c.type === "line" && c.style.stroke === grid);
    expect(spokes).toHaveLength(24);
    expect(
      spokes
        .map((c) => Math.round((Math.atan2(-c.shape.y2, c.shape.x2) * 180) / Math.PI + 360) % 360)
        .sort((a, b) => a - b),
    ).toEqual(Array.from({ length: 24 }, (_, i) => i * 15));
    for (const spoke of spokes) {
      expect(spoke.shape.x1).toBeCloseTo(0, 9);
      expect(spoke.shape.y1).toBeCloseTo(0, 9);
    }
  });

  it("gives the colour bar a labelled scale from the smallest positive cell to the largest", () => {
    const option = polarHeatmapOption(vibrationsResult()) as Loose;
    // The smallest positive cell is 1 and the largest 18; the zero cell is clamped to the smallest.
    expect(option.visualMap.min).toBe(0);
    expect(option.visualMap.max).toBeCloseTo(Math.log10(18), 9);
    expect(option.visualMap.text).toEqual(["18", "1"]);
  });

  it("copes with a grid without a positive cell", () => {
    const r = vibrationsResult({
      grid: [
        [0, 0],
        [-1, 0],
      ],
      speeds: [1, 2],
      angles: [0, 180],
    });
    assertFinite(polarHeatmapOption(r));
    assertFinite(heatmapOption(r));
  });

  it("draws the bitmap once, over the polar plot, and nothing for the other data row", () => {
    const written = stubCanvas();
    const item = series(polarHeatmapOption(vibrationsResult()))[0].renderItem;
    const coordSys = { cx: 300, cy: 200, r: 150 };
    expect(item({ dataIndex: 1, coordSys })).toBeNull();
    const drawn = item({ dataIndex: 0, coordSys });
    expect(drawn.type).toBe("image");
    expect(drawn.style).toMatchObject({ x: 150, y: 50, width: 300, height: 300 });
    item({ dataIndex: 0, coordSys });
    expect(written).toHaveLength(1);
  });

  it("colours each pixel by the cell at its angle and speed, and leaves the corners clear", () => {
    const written = stubCanvas();
    series(polarHeatmapOption(vibrationsResult()))[0].renderItem({ dataIndex: 0, coordSys: { cx: 0, cy: 0, r: 1 } });
    const bitmap = written[0];
    expect(bitmap.width).toBe(720);
    expect(pixel(bitmap, 0, 0)[3]).toBe(0);
    // Just right of the centre is angle 0 at the lowest speed: the clamped zero cell, the darkest colour.
    expect(pixel(bitmap, 361, 359)).toEqual([0, 0, 4, 255]);
    expect(pixel(bitmap, 700, 359)[0]).toBeGreaterThan(0);
  });
});

describe("speedProfileOption", () => {
  it("has min, max, variance and metric series with a dashed variance and a hidden second axis", () => {
    const option = speedProfileOption(vibrationsResult()) as Loose;
    const names = series(option).map((s) => s.name);
    expect(names).toEqual(["Minimum", "Maximum", "Variance", "Vibration metric (2 bad peaks)"]);
    expect(series(option)[2].lineStyle.type).toBe("dashed");
    expect(option.yAxis).toHaveLength(2);
    expect(option.yAxis[1].show).toBe(false);
    expect(series(option)[3].yAxisIndex).toBe(1);
  });

  it("marks each peak with a small circle and its number, as the shaper chart does", () => {
    const metric = series(speedProfileOption(vibrationsResult()))[3];
    expect(metric.markPoint.symbol).toBe("circle");
    expect(metric.markPoint.symbolSize).toBe(6);
    expect(metric.markPoint.data).toHaveLength(2);
    expect(metric.markPoint.data[0].label.formatter).toBe("{index|1}");
    expect(metric.markPoint.data[1].label.formatter).toBe("{index|2}");
  });

  it("shades one zone per good speed range, numbered with the green badge of its table row", () => {
    const r = vibrationsResult({
      good_speeds: [
        { start: 0, end: 1, energy: 1 },
        { start: 2, end: 3, energy: 2 },
      ],
    });
    const area = series(speedProfileOption(r))[0].markArea.data as Loose[][];
    expect(area).toHaveLength(2);
    expect(area[0][0]).toMatchObject({ name: "{zone|1}", xAxis: 20 });
    expect(area[0][1]).toMatchObject({ xAxis: 40 });
    expect(area[1][0]).toMatchObject({ name: "{zone|2}", xAxis: 60 });
    expect(area[1][1]).toMatchObject({ xAxis: 80 });
    const label = series(speedProfileOption(r))[0].markArea.label;
    expect(label.rich.zone.backgroundColor).toBe(GOOD_COLOR);
  });

  it("colours a bad speed with the accent and a good range green, so a number is never ambiguous", () => {
    const r = vibrationsResult();
    const peak = series(speedProfileOption(r))[3].markPoint.data[0].label.rich.index.backgroundColor;
    const zone = series(speedProfileOption(r))[0].markArea.label.rich.zone.backgroundColor;
    const line = (series(heatmapOption(r))[1].markLine.data[0].label as Loose).color;
    expect(peak).toBe(PEAK_COLOR);
    expect(line).toBe(PEAK_COLOR);
    expect(zone).toBe(GOOD_COLOR);
    expect(peak).not.toBe(zone);
    // The motor resonance is neither, so it keeps a neutral badge.
    const motor = series(motorProfileOption(motorResult(), 100, "corexy"))[0].markPoint.data[0].label.rich.index;
    expect([PEAK_COLOR, GOOD_COLOR]).not.toContain(motor.backgroundColor);
  });
});

describe("angularProfilesOption", () => {
  it("plots the rows nearest 0, 90, 45 and 135 degrees with corexy names", () => {
    const r = vibrationsResult();
    const s = series(angularProfilesOption(r, "corexy"));
    expect(s.map((x) => x.name)).toEqual(["X (0 deg)", "Y (90 deg)", "A (45 deg)", "B (135 deg)"]);
    expect(s[0].data.map((d: number[]) => d[1])).toEqual(r.grid[0]);
    expect(s[1].data.map((d: number[]) => d[1])).toEqual(r.grid[2]);
    expect(s[2].data.map((d: number[]) => d[1])).toEqual(r.grid[1]);
    expect(s[3].data.map((d: number[]) => d[1])).toEqual(r.grid[3]);
  });

  it("names the diagonals plainly for other kinematics", () => {
    const names = series(angularProfilesOption(vibrationsResult(), "cartesian")).map((x) => x.name);
    expect(names).toEqual(["X (0 deg)", "Y (90 deg)", "45 deg", "135 deg"]);
    const limited = series(angularProfilesOption(vibrationsResult(), "limited_corexy")).map((x) => x.name);
    expect(limited[2]).toBe("A (45 deg)");
  });
});

describe("heatmapOption", () => {
  it("has speed on x and angle from 0 to 360 on y, as one bitmap with a labelled colour bar", () => {
    const option = heatmapOption(vibrationsResult()) as Loose;
    expect(option.xAxis).toMatchObject({ type: "value", min: 0, max: 80, interval: 25 });
    expect(option.yAxis).toMatchObject({ type: "value", min: 0, max: 360 });
    expect(series(option)[0].type).toBe("custom");
    expect(option.visualMap.text).toEqual(["18", "1"]);
  });

  it("places the bitmap between the first and last speed, with angle 0 at the bottom", () => {
    const written = stubCanvas();
    const r = vibrationsResult();
    const item = series(heatmapOption(r))[0].renderItem;
    // A plot 300 wide for 0 to 100 mm/s and 400 high for 0 to 360 degrees, pixel y growing downwards.
    const api = { coord: ([speed, angle]: number[]) => [10 + speed * 3, 20 + 400 - (angle / 360) * 400] };
    expect(item({ dataIndex: 1 }, api)).toBeNull();
    // The measured speeds are 20 to 80, so the bitmap spans x 70 to 250 and the whole angle range.
    expect(item({ dataIndex: 0 }, api).style).toMatchObject({ x: 70, y: 20, width: 180, height: 400 });
    const bitmap = written[0];
    expect([bitmap.width, bitmap.height]).toEqual([4, 9]);
    // Bottom left is angle 0 at the lowest speed, the clamped zero cell: the darkest colour.
    expect(pixel(bitmap, 0, 8)).toEqual([0, 0, 4, 255]);
    // Top right is angle 360 at the highest speed, the largest cell: the brightest colour.
    expect(pixel(bitmap, 3, 0)).toEqual([252, 255, 164, 255]);
  });

  it("draws an accent line at the speed of each bad peak, labelled along it with the number and the speed", () => {
    const lines = series(heatmapOption(vibrationsResult()))[1].markLine.data as Loose[];
    expect(lines).toHaveLength(2);
    expect(lines.map((l) => l.xAxis)).toEqual([40, 80]);
    expect(lines.map((l) => l.label.formatter)).toEqual(["Peak 1 (40 mm/s)", "Peak 2 (80 mm/s)"]);
    // Along the line, so that ten of them do not overlap.
    expect(lines[0].label.position).toBe("insideEndTop");
    expect(lines[0].lineStyle.color).toBe(PEAK_COLOR);
  });

  it("reads the speed and angle under the pointer without a tooltip box", () => {
    const option = heatmapOption(vibrationsResult()) as Loose;
    expect(option.tooltip).toMatchObject({ trigger: "axis", showContent: false });
  });
});

describe("motorProfileOption", () => {
  it("plots the combined profile and a dashed line per angle, marking the resonance", () => {
    const m = motorResult();
    const s = series(motorProfileOption(m, 100, "corexy"));
    expect(s).toHaveLength(3);
    expect(s[1].name).toBe("X (0 deg)");
    expect(s[2].name).toBe("Y (90 deg)");
    expect(s[1].lineStyle.type).toBe("dashed");
    expect(s[0].markPoint.data).toHaveLength(1);
    expect(s[0].markPoint.data[0].coord).toEqual([30, 4.5]);
    expect(s[0].markPoint.data[0].name).toBe("R");
    expect(s[0].markPoint.symbolSize).toBe(6);
    expect(s[0].markPoint.data[0].label.formatter).toBe("{index|R}");
  });

  it("keeps the resonance and damping out of the chart, where they collided with the axis label", () => {
    const text = JSON.stringify(motorProfileOption(motorResult({ low_freq_warning: true }), 100, "corexy"));
    expect(text).not.toContain("resonant frequency");
    expect(text).not.toContain("damping");
    expect(text).not.toContain(LOW_FREQ_WARNING);
    expect((motorProfileOption(motorResult(), 100, "corexy") as Loose).title).toBeUndefined();
  });

  it("limits the x axis to the maximum frequency", () => {
    expect((motorProfileOption(motorResult(), 80, "corexy") as Loose).xAxis.max).toBe(80);
  });
});

describe("siEnergy", () => {
  it("writes an energy with a short SI suffix", () => {
    expect(siEnergy(22_600_000)).toBe("22.6M");
    expect(siEnergy(801_000)).toBe("801k");
    expect(siEnergy(1234)).toBe("1.23k");
    expect(siEnergy(5)).toBe("5");
    expect(siEnergy(3.2e9)).toBe("3.2G");
    expect(siEnergy(0)).toBe("0");
    expect(siEnergy(Number.NaN)).toBe("0");
  });
});

describe("every vibrations option", () => {
  it("holds no NaN or infinity", () => {
    for (const option of allOptions()) assertFinite(option);
  });

  it("is deterministic", () => {
    expect(JSON.stringify(allOptions())).toBe(JSON.stringify(allOptions()));
  });

  it("renders with the registered chart parts", () => {
    stubCanvas();
    echarts.use([...CHART_PARTS, SVGRenderer]);
    for (const option of allOptions()) {
      const chart = echarts.init(null, null, { renderer: "svg", ssr: true, width: 600, height: 400 });
      chart.setOption(option as Parameters<echarts.ECharts["setOption"]>[0]);
      expect(chart.renderToSVGString()).toContain("<svg");
      chart.dispose();
    }
  });
});
