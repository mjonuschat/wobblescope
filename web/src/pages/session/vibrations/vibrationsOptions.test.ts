import * as echarts from "echarts/core";
import { SVGRenderer } from "echarts/renderers";
import { describe, expect, it } from "vitest";

import { CHART_PARTS } from "@/components/charts/register";
import { linspaceVibrationsResult, motorResult, vibrationsResult } from "@/test/vibrations";

import {
  angularProfilesOption,
  heatmapOption,
  motorProfileOption,
  polarAngleOption,
  polarHeatmapOption,
  speedProfileOption,
} from "./vibrationsOptions";

const round1 = (value: number) => Number(value.toFixed(1));

// biome-ignore lint/suspicious/noExplicitAny: option objects are inspected loosely
type Loose = Record<string, any>;

const series = (option: unknown): Loose[] => (option as Loose).series;

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
    const main = series(option)[0];
    expect(main.type).toBe("line");
    expect(main.coordinateSystem).toBe("polar");
    expect(main.areaStyle).toBeDefined();
    expect(main.data).toHaveLength(9);
  });

  it("shows the symmetry, or that it is not available", () => {
    expect(JSON.stringify(polarAngleOption(vibrationsResult()))).toContain("Symmetry: 87.7%");
    expect(JSON.stringify(polarAngleOption(vibrationsResult({ symmetry: null })))).toContain("Symmetry: not available");
  });

  it("shades one sector per good angle zone", () => {
    const sectors = series(polarAngleOption(vibrationsResult())).slice(1);
    expect(sectors).toHaveLength(1);
    expect(sectors[0].areaStyle).toBeDefined();
    const arc = sectors[0].data as number[][];
    expect(arc[1][1]).toBe(45);
    expect(arc[arc.length - 2][1]).toBe(135);
  });

  it("spans angles[start] to angles[end] on linspace angles, out to the radius axis maximum", () => {
    const r = linspaceVibrationsResult({ good_angles: [{ start: 100, end: 300, energy: 1 }] });
    const option = polarAngleOption(r) as Loose;
    const arc = series(option)[1].data as number[][];
    expect(arc[1][1]).toBeCloseTo(r.angles[100], 9);
    expect(arc[arc.length - 2][1]).toBeCloseTo(r.angles[300], 9);
    expect(arc[1][0]).toBe(option.radiusAxis.max);
    expect(option.radiusAxis.max).toBe(Math.max(...r.angle_energy));
    expect(series(polarAngleOption(vibrationsResult({ good_angles: [] })))).toHaveLength(1);
  });
});

describe("polarHeatmapOption", () => {
  it("is a polar heatmap with angle around and speed as radius, on a log scale", () => {
    const option = polarHeatmapOption(vibrationsResult()) as Loose;
    expect(option.polar).toBeDefined();
    expect(series(option)[0]).toMatchObject({ type: "heatmap", coordinateSystem: "polar" });
    expect(option.visualMap).toBeDefined();
  });

  it("clamps a zero cell to the smallest positive value", () => {
    const data = series(polarHeatmapOption(vibrationsResult()))[0].data as number[][];
    const values = data.map((d) => d[2]);
    // The smallest positive kept cell is 1, so log10 is 0, and the zero cell is clamped to it.
    expect(Math.min(...values)).toBe(0);
    expect(data[0][2]).toBe(0);
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

  it("keeps every other angle row and every speed column", () => {
    const r = vibrationsResult();
    const data = series(polarHeatmapOption(r))[0].data as number[][];
    expect(data).toHaveLength(Math.ceil(r.angles.length / 2) * r.speeds.length);
    const rows = new Set(data.map((d) => d[1]));
    expect(rows.size).toBe(5);
  });

  it("downsamples 720 angle rows to 360 and keeps every other row", () => {
    const r = linspaceVibrationsResult();
    const option = polarHeatmapOption(r) as Loose;
    const data = option.series[0].data as number[][];
    expect(r.angles).toHaveLength(720);
    expect(new Set(data.map((d) => d[1])).size).toBe(360);
    expect(data).toHaveLength(360 * r.speeds.length);
    expect(option.angleAxis.data).toHaveLength(360);
    expect(option.angleAxis.data[1]).toBe(String(round1(r.angles[2])));
  });

  it("shows the radius as speeds in mm/s", () => {
    const r = linspaceVibrationsResult();
    const option = polarHeatmapOption(r) as Loose;
    expect(option.radiusAxis.name).toBe("Speed (mm/s)");
    expect(option.radiusAxis.data).toEqual(["10", "25", "40", "55", "70", "85", "100"]);
    expect(typeof option.radiusAxis.axisLabel.interval).toBe("number");
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

  it("marks each peak with its number", () => {
    const metric = series(speedProfileOption(vibrationsResult()))[3];
    expect(metric.markPoint.data).toHaveLength(2);
    expect(JSON.stringify(metric.markPoint.data)).toContain('"1"');
    expect(JSON.stringify(metric.markPoint.data)).toContain('"2"');
  });

  it("shades one labelled zone per good speed range", () => {
    const r = vibrationsResult({
      good_speeds: [
        { start: 0, end: 1, energy: 1 },
        { start: 2, end: 3, energy: 2 },
      ],
    });
    const area = series(speedProfileOption(r))[0].markArea.data as Loose[][];
    expect(area).toHaveLength(2);
    expect(area[0][0].name).toBe("Zone 1: 20 to 40 mm/s");
    expect(area[1][0].name).toBe("Zone 2: 60 to 80 mm/s");
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
  it("has speed on x and angle on y and a downsampled grid", () => {
    const r = vibrationsResult();
    const option = heatmapOption(r) as Loose;
    expect(series(option)[0].type).toBe("heatmap");
    expect(series(option)[0].data).toHaveLength(5 * 4);
    expect(option.xAxis.data).toHaveLength(4);
    expect(option.yAxis.data).toHaveLength(5);
  });

  it("draws a labelled cyan line per bad peak", () => {
    const lines = series(heatmapOption(vibrationsResult()))[0].markLine.data as Loose[];
    expect(lines).toHaveLength(2);
    expect(lines[0].label.formatter).toBe("Peak 1 (40 mm/s)");
    expect(lines[1].label.formatter).toBe("Peak 2 (80 mm/s)");
    expect(lines[0].lineStyle.color).toMatch(/cyan|#00/i);
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
  });

  it("states the resonance and damping, or that no damping was computed", () => {
    const text = (o: unknown) => JSON.stringify(o);
    expect(text(motorProfileOption(motorResult(), 100, "corexy"))).toContain("Motor resonant frequency (w0): 30.0 Hz");
    expect(text(motorProfileOption(motorResult(), 100, "corexy"))).toContain("Motor damping ratio (zeta): 0.123");
    expect(text(motorProfileOption(motorResult({ zeta: null }), 100, "corexy"))).toContain("No damping ratio computed");
  });

  it("warns about low frequency vibrations only when flagged", () => {
    const text = (o: unknown) => JSON.stringify(o);
    expect(text(motorProfileOption(motorResult(), 100, "corexy"))).not.toContain("too high an acceleration");
    expect(text(motorProfileOption(motorResult({ low_freq_warning: true }), 100, "corexy"))).toContain(
      "too high an acceleration",
    );
  });

  it("limits the x axis to the maximum frequency", () => {
    expect((motorProfileOption(motorResult(), 80, "corexy") as Loose).xAxis.max).toBe(80);
  });
});

const DECIMALS = /\d+\.\d{2,}/;

function labelStrings(result: ReturnType<typeof vibrationsResult>): string[] {
  const strings: string[] = [];
  const polar = polarHeatmapOption(result) as Loose;
  const flat = heatmapOption(result) as Loose;
  strings.push(...polar.angleAxis.data, ...polar.radiusAxis.data, ...flat.xAxis.data, ...flat.yAxis.data);
  for (const option of [polar, flat]) {
    for (const [s, a] of [
      [0, 0],
      [1, 1],
      [result.speeds.length - 1, Math.floor(result.angles.length / 2) - 1],
    ]) {
      strings.push(option.tooltip.formatter({ value: [s, a, 0] }));
    }
  }
  return strings;
}

describe("angle and speed labels", () => {
  it("are short on linspace angles and speeds", () => {
    const strings = labelStrings(linspaceVibrationsResult());
    expect(strings.length).toBeGreaterThan(700);
    for (const text of strings) expect(text, text).not.toMatch(DECIMALS);
    expect((polarHeatmapOption(linspaceVibrationsResult()) as Loose).angleAxis.data[1]).toBe("1");
  });

  it("are short on the exact fixture too", () => {
    for (const text of labelStrings(vibrationsResult())) expect(text, text).not.toMatch(DECIMALS);
  });

  it("show speeds in mm/s on the heatmap speed axis", () => {
    const option = heatmapOption(linspaceVibrationsResult()) as Loose;
    expect(option.xAxis.data).toEqual(["10", "25", "40", "55", "70", "85", "100"]);
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
    echarts.use([...CHART_PARTS, SVGRenderer]);
    for (const option of allOptions()) {
      const chart = echarts.init(null, null, { renderer: "svg", ssr: true, width: 600, height: 400 });
      chart.setOption(option as Parameters<echarts.ECharts["setOption"]>[0]);
      expect(chart.renderToSVGString()).toContain("<svg");
      chart.dispose();
    }
  });
});
