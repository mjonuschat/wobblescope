import { describe, expect, it } from "vitest";

import { DEFAULT_CHART_THEME } from "@/ui-kit/chartTheme";

import { bandOption } from "./bandOption";

type Loose = Record<string, any>;

describe("bandOption", () => {
  const series = [
    { chip: "a", color: "rgb(1, 2, 3)", points: [[-2, 5]] as [number, number][] },
    { chip: "b", color: "rgb(4, 5, 6)", points: [[-1, 7]] as [number, number][] },
  ];

  it("draws one line per chip in the chip's colour, from the span ago to now", () => {
    const option = bandOption(series, 60) as Loose;
    expect(option.series.map((s: Loose) => [s.name, s.lineStyle.color, s.data])).toEqual([
      ["a", "rgb(1, 2, 3)", [[-2, 5]]],
      ["b", "rgb(4, 5, 6)", [[-1, 7]]],
    ]);
    expect(option.xAxis).toMatchObject({ min: -60, max: 0 });
    expect(option.yAxis).toMatchObject({ min: 0, name: "mm/s² RMS" });
  });

  it("has a legend only when there is more than one chip, and takes its colours from the theme", () => {
    expect((bandOption(series, 60) as Loose).legend.type).toBe("scroll");
    expect((bandOption(series.slice(0, 1), 60) as Loose).legend).toBeUndefined();
    expect((bandOption(series, 60, DEFAULT_CHART_THEME) as Loose).textStyle.color).toBe(DEFAULT_CHART_THEME.text);
  });
});
