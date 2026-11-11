import { describe, expect, it } from "vitest";

import { analysis, candidate, recommendation } from "@/test/fixtures";

import { pointLabel, psdOption, summedOption } from "./options";

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
    const selected = (option as { legend: { selected: Record<string, boolean> } }).legend.selected;
    expect(Object.keys(selected).sort()).toEqual(["X", "Y", "Z"]);
    expect(Object.values(selected).every((v) => v === false)).toBe(true);
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
  it("has one series and says in its title that it is for display only", () => {
    const option = summedOption(analysis().summed!, 200);
    expect(series(option)).toHaveLength(1);
    expect((option as { title: { text: string } }).title.text).toContain("display only");
  });
});
