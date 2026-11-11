import { describe, expect, it } from "vitest";

import { chipColor } from "@/pages/stream/colors";
import { analysis, candidate, recommendation } from "@/test/fixtures";

import { liveSpectrumOption, pointLabel, psdOption, summedOption } from "./options";

type Series = { name: string; yAxisIndex?: number; markLine?: { data: { xAxis: number }[] } };

const series = (o: unknown) => (o as { series: Series[] }).series;

describe("psdOption", () => {
  const chip = analysis().per_chip[0];
  const rec = recommendation(candidate("mzv", 48.2));
  const option = psdOption({
    chip,
    recommendation: rec,
    maxFreq: 200,
  });

  it("draws one total series per probe point and keeps X, Y and Z hidden in the legend", () => {
    const left = series(option).filter((s) => s.yAxisIndex === undefined);
    expect(left.map((s) => s.name)).toEqual(["Total", "X", "Y", "Z"]);
    const selected = (option as { legend: { selected: Record<string, boolean> }[] }).legend[0].selected;
    expect(Object.keys(selected).sort()).toEqual(["X", "Y", "Z"]);
    expect(Object.values(selected).every((v) => v === false)).toBe(true);
  });

  it("puts the measured curves and the shapers in a legend of their own each", () => {
    const legends = (option as { legend: { data: string[] }[] }).legend;
    expect(legends).toHaveLength(2);
    expect(legends[0].data).toEqual(["Total", "X", "Y", "Z"]);
    expect(legends[1].data).toEqual(rec.all.map((c) => c.name));
  });

  it("has only the measured legend without a recommendation", () => {
    const bare = psdOption({ chip, recommendation: null, maxFreq: 200 });
    expect((bare as { legend: unknown[] }).legend).toHaveLength(1);
  });

  it("names each shaper curve by the shaper alone", () => {
    const names = series(option)
      .filter((s) => s.yAxisIndex === 1)
      .map((s) => s.name);
    expect(names).toEqual(rec.all.map((c) => c.name));
  });

  it("draws one right-axis series per fitted shaper and marks the recommended frequency", () => {
    expect(series(option).filter((s) => s.yAxisIndex === 1)).toHaveLength(rec.all.length);
    const marked = series(option).flatMap((s) => s.markLine?.data ?? []);
    expect(marked.map((m) => m.xAxis)).toEqual([48.2]);
  });

  it("ends the frequency axis at the fit settings\u2019 maximum", () => {
    expect((option as { xAxis: { max: number } }).xAxis.max).toBe(200);
  });

  it("has no marker and no response curves without a recommendation", () => {
    const bare = psdOption({
      chip,
      recommendation: null,
      maxFreq: 200,
    });
    expect(series(bare).some((s) => s.markLine || s.yAxisIndex === 1)).toBe(false);
  });

  it("plots the curves exactly as core returned them", () => {
    const total = series(option)[0] as unknown as { data: number[][] };
    expect(total.data).toEqual([
      [5, 3],
      [5.2, 4],
      [5.4, 5],
    ]);
  });
});

describe("psdOption with several test points", () => {
  it("numbers the curves by point, from 1, and uses no coordinates", () => {
    const one = analysis().per_chip[0];
    const chip = { ...one, datasets: [one.datasets[0], [1, one.datasets[0][1]] as (typeof one.datasets)[number]] };
    const names = series(psdOption({ chip, recommendation: null, maxFreq: 200 }))
      .filter((s) => s.yAxisIndex === undefined)
      .map((s) => s.name);
    expect(names).toContain("Point 1 Total");
    expect(names).toContain("Point 2 Total");
    expect(names).toContain("Point 2 Z");
    expect(names.some((n) => n.includes("("))).toBe(false);
  });
});

describe("psdOption shaper curves", () => {
  it("plots each shaper on the recommendation grid, up to its own last value", () => {
    const short = candidate("zv", 55, { vals: [0.9, 0.5] });
    const rec = { ...recommendation(), all: [short, candidate("mzv", 48.2)] };
    const option = psdOption({
      chip: analysis().per_chip[0],
      recommendation: rec,
      maxFreq: 200,
    });
    const curves = series(option).filter((s) => s.yAxisIndex === 1) as unknown as { data: number[][] }[];
    expect(curves[0].data).toEqual([
      [5, 0.9],
      [5.2, 0.5],
    ]);
    expect(curves[1].data).toEqual([
      [5, 0.9],
      [5.2, 0.5],
      [5.4, 0.1],
    ]);
  });
});

describe("pointLabel", () => {
  it("names a point by its position, or by its number when the position is unknown", () => {
    const points = [{ position: null }, { position: [100, 100, 20] as [number, number, number] }];
    expect(pointLabel(0, points)).toBe("point 0");
    expect(pointLabel(1, points)).toBe("(100, 100, 20)");
    expect(pointLabel(2, points)).toBe("point 2");
  });
});

describe("summedOption", () => {
  const option = summedOption(analysis().summed!, 200);

  it("has no heading of its own and a plain axis name", () => {
    expect((option as { title?: unknown }).title).toBeUndefined();
    expect((option as { yAxis: { name: string } }).yAxis.name).toBe("Vibration Strength");
  });

  it("draws the total and the three directions, with the directions hidden in the legend", () => {
    expect(series(option).map((s) => s.name)).toEqual(["Total", "X", "Y", "Z"]);
    const legend = (option as { legend: { selected: Record<string, boolean> } }).legend;
    expect(legend.selected).toEqual({ X: false, Y: false, Z: false });
  });

  it("plots each direction from its own summed curve", () => {
    const x = series(option)[1] as unknown as { data: number[][] };
    expect(x.data).toEqual([
      [5, 2],
      [5.2, 3],
      [5.4, 4],
    ]);
  });
});

describe("liveSpectrumOption", () => {
  const spectrum = {
    freq_bins: [0, 10, 20],
    psd_x: [1, 2, 3],
    psd_y: [1, 2, 3],
    psd_z: [1, 2, 3],
    psd_sum: [3, 6, 9],
  };
  const peaks = [{ freq: 10, level: 6 }];
  const option = liveSpectrumOption({
    chips: [
      { chip: "a", spectrum, peaks, peakHold: [3, 7, 9], axes: ["x", "sum"] },
      { chip: "b", spectrum, peaks: [], peakHold: [3, 6, 9], axes: ["z"] },
    ],
    maxHz: 200,
  });
  type Live = {
    name: string;
    color: string;
    lineStyle: { color: string; opacity?: number };
    markPoint?: { data: { coord: number[]; label: { formatter: string } }[] };
  };
  const live = (option as { series: Live[] }).series;
  const byName = (name: string) => live.find((s) => s.name === name)!;

  it("draws a line per chip and axis plus a peak-hold line per chip", () => {
    expect(live.map((s) => s.name)).toEqual(["a X", "a Total", "a Peak Hold", "b Z", "b Peak Hold"]);
  });

  it("colours every line of a chip with that chip\u2019s colour", () => {
    for (const name of ["a X", "a Total", "a Peak Hold"]) {
      expect(byName(name).color).toBe(chipColor(0));
      expect(byName(name).lineStyle.color).toBe(chipColor(0));
    }
    for (const name of ["b Z", "b Peak Hold"]) expect(byName(name).lineStyle.color).toBe(chipColor(1));
  });

  it("colours by colorIndex when given, not by position", () => {
    const picked = liveSpectrumOption({
      chips: [{ chip: "b", colorIndex: 1, spectrum, peaks: [], peakHold: [], axes: ["sum"] }],
      maxHz: 200,
    });
    expect((picked as { series: Live[] }).series[0].lineStyle.color).toBe(chipColor(1));
  });

  it("draws the peak-hold lines faintly and the spectra solidly", () => {
    expect(byName("a Peak Hold").lineStyle.opacity).toBe(0.3);
    expect(byName("b Peak Hold").lineStyle.opacity).toBe(0.3);
    expect(byName("a X").lineStyle.opacity).toBeUndefined();
  });

  it("marks peaks on the first line of a chip only, labelled with Hz and level", () => {
    const [point] = byName("a X").markPoint!.data;
    expect(point.coord).toEqual([10, 6]);
    expect(point.label.formatter).toBe("10.0 Hz\n6.0e+0");
    expect(byName("a Total").markPoint).toBeUndefined();
    expect(byName("a Peak Hold").markPoint).toBeUndefined();
    expect(byName("b Z").markPoint?.data).toEqual([]);
  });

  it("uses an axis tooltip and the requested frequency limit", () => {
    expect((option as { tooltip: { trigger: string } }).tooltip.trigger).toBe("axis");
    expect((option as { xAxis: { max: number } }).xAxis.max).toBe(200);
  });
});
