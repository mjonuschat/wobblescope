import { describe, expect, it } from "vitest";

import { DEFAULT_CHART_THEME } from "@/ui-kit/chartTheme";

import { bandOption } from "./bandOption";

interface BandChart {
  series: { name: string; lineStyle: { color: string }; data: [number, number][] }[];
  xAxis: { min: number; max: number };
  yAxis: { min: number; name: string; axisLine: { onZero: boolean } };
  legend?: { type: string };
  textStyle: { color: string };
}
const chart = (option: unknown) => option as BandChart;

describe("bandOption", () => {
  const series = [
    { chip: "a", color: "rgb(1, 2, 3)", points: [[-2, 5]] as [number, number][] },
    { chip: "b", color: "rgb(4, 5, 6)", points: [[-1, 7]] as [number, number][] },
  ];

  it("draws one line per chip in the chip's colour, from the span ago to now", () => {
    const option = chart(bandOption(series, 60));
    expect(option.series.map((s) => [s.name, s.lineStyle.color, s.data])).toEqual([
      ["a", "rgb(1, 2, 3)", [[-2, 5]]],
      ["b", "rgb(4, 5, 6)", [[-1, 7]]],
    ]);
    expect(option.xAxis).toMatchObject({ min: -60, max: 0 });
    expect(option.yAxis).toMatchObject({ min: 0, name: "mm/s² RMS" });
    // Now sits at x = 0, so without this the y axis would be drawn at the right end.
    expect(option.yAxis.axisLine.onZero).toBe(false);
  });

  it("has a legend only when there is more than one chip, and takes its colours from the theme", () => {
    expect(chart(bandOption(series, 60)).legend?.type).toBe("scroll");
    expect(chart(bandOption(series.slice(0, 1), 60)).legend).toBeUndefined();
    expect(chart(bandOption(series, 60, DEFAULT_CHART_THEME)).textStyle.color).toBe(DEFAULT_CHART_THEME.text);
  });
});
