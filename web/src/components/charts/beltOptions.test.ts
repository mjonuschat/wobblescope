import { describe, expect, it } from "vitest";

import { beltSpectrum, beltsResult } from "@/test/fixtures";
import { DEFAULT_CHART_THEME } from "@/ui-kit/chartTheme";

import { beltOverlayOption, beltScatterOption, beltSingleOption, goodZoneBoundary } from "./beltOptions";

const T = DEFAULT_CHART_THEME;

type Mark = {
  coord: number[];
  itemStyle?: { color: string; borderWidth?: number };
  label: {
    formatter: string;
    rich: { badge: { backgroundColor: string; color: string; fontWeight: string; borderRadius: number } };
  };
};
type BeltSeries = {
  name: string;
  stack?: string;
  data: number[][];
  markPoint?: { data: Mark[] };
  markLine?: { data: { coord: number[] }[][] };
};

describe("beltOverlayOption", () => {
  it("the_y_axis_name_sits_beside_the_axis_inside_the_grid_margin_not_at_the_legend", () => {
    const option = beltOverlayOption(beltsResult()) as unknown as {
      yAxis: { nameLocation: string; nameGap: number };
      grid: { left: number };
    };
    expect(option.yAxis.nameLocation).toBe("middle");
    expect(option.yAxis.nameGap).toBeLessThan(option.grid.left);
  });

  it("the_overlay_has_two_curves_and_marks_pairs_with_greek_letters_and_unpaired_with_numbers", () => {
    const option = beltOverlayOption(beltsResult());
    const [a, b] = (option as { series: BeltSeries[] }).series;
    expect([a.name, b.name]).toEqual(["Belt A", "Belt B"]);
    expect(a.data).toEqual([
      [0, 0],
      [50, 4],
      [100, 1],
      [150, 6],
      [200, 0],
    ]);
    expect(a.markPoint?.data.map((m) => m.label.formatter)).toEqual(["{badge|A α}", "{badge|A β}"]);
    expect(b.markPoint?.data.map((m) => m.label.formatter)).toEqual(["{badge|B α}", "{badge|B β}", "{badge|B 1}"]);
    expect(b.markPoint?.data[2]?.coord).toEqual([150, 5.5]);
    expect(a.markLine?.data).toEqual([
      [{ coord: [50, 4] }, { coord: [50, 3] }],
      [{ coord: [150, 6] }, { coord: [100, 5] }],
    ]);
    expect((option as { xAxis: { max: number } }).xAxis.max).toBe(200);
  });

  it("fills_paired_badges_with_the_belt_colour_and_unpaired_ones_red_with_dark_text", () => {
    const [a, b] = (beltOverlayOption(beltsResult()) as { series: BeltSeries[] }).series;
    const fills = (series: BeltSeries) => series.markPoint?.data.map((m) => m.label.rich.badge.backgroundColor);
    expect(fills(a)).toEqual([T.accent, T.accent]);
    expect(fills(b)).toEqual([T.series[0], T.series[0], T.danger]);
    for (const mark of [...(a.markPoint?.data ?? []), ...(b.markPoint?.data ?? [])]) {
      expect(mark.label.rich.badge).toMatchObject({
        color: T.onFill(mark.label.rich.badge.backgroundColor),
        fontWeight: "bold",
        borderRadius: 3,
      });
    }
  });

  it("colours_the_marker_dots_by_belt_without_an_outline", () => {
    const [a, b] = (beltOverlayOption(beltsResult()) as unknown as { series: { markPoint: { itemStyle: object } }[] })
      .series;
    expect(a.markPoint.itemStyle).toEqual({ color: T.accent, borderWidth: 0 });
    expect(b.markPoint.itemStyle).toEqual({ color: T.series[0], borderWidth: 0 });
  });

  it("the_y_axis_fits_the_data", () => {
    expect((beltOverlayOption(beltsResult()) as { yAxis: { max: number } }).yAxis.max).toBeCloseTo(6.6);
  });
});

describe("beltSingleOption", () => {
  it("draws one curve without peak marks", () => {
    const option = beltSingleOption(beltSpectrum(), "A (axis 1,-1)");
    const list = (option as { series: BeltSeries[] }).series;
    expect(list).toHaveLength(1);
    expect(list[0].markPoint).toBeUndefined();
    expect(list[0].data).toHaveLength(5);
    expect((option as { yAxis: { max: number } }).yAxis.max).toBeCloseTo(6.6);
  });
});

describe("beltScatterOption", () => {
  const option = beltScatterOption(beltsResult());
  const list = (option as { series: BeltSeries[] }).series;
  const named = (name: string) => list.find((s) => s.name === name)!;

  it("the_scatter_has_the_ideal_line_and_the_good_zone", () => {
    const end = 6 * 1.1;
    expect(named("Ideal line").data).toEqual([
      [0, 0],
      [end, end],
    ]);
    const floor = named("Good zone floor");
    const zone = named("Good zone");
    expect(zone.stack).toBe(floor.stack);
    zone.data.forEach(([x, height], i) => {
      expect(floor.data[i][0]).toBe(x);
      expect(floor.data[i][1] + height).toBeCloseTo(x + 0.35 * 6 * Math.exp(-x / (0.6 * 6)));
      expect(floor.data[i][1] + height).toBeCloseTo(goodZoneBoundary(x, 6));
    });
  });

  it("shades_between_the_curve_and_the_y_axis", () => {
    const shading = (
      list as unknown as { name: string; renderItem: (p: unknown, api: unknown) => { shape: { points: number[][] } } }[]
    ).find((s) => s.name === "Belt curve shading")!;
    const { shape } = shading.renderItem(null, { coord: (point: number[]) => point });
    expect(shape.points).toEqual([
      [0, 0],
      [4, 3],
      [1, 5],
      [6, 5.5],
      [0, 0],
      [0, 0],
      [0, 0],
    ]);
  });

  it("formats_both_scatter_axes_like_the_overlay_y_axis", () => {
    const axes = option as {
      xAxis: { axisLabel: { formatter: (v: number) => string } };
      yAxis: { axisLabel: { formatter: (v: number) => string } };
    };
    const overlay = beltOverlayOption(beltsResult()) as unknown as typeof axes;
    expect(axes.xAxis.axisLabel.formatter(12345)).toBe(overlay.yAxis.axisLabel.formatter(12345));
    expect(axes.yAxis.axisLabel.formatter(0.5)).toBe("5.0e-1");
  });

  it("plots belt A against belt B and marks the pairs", () => {
    expect(named("Belts").data).toEqual([
      [0, 0],
      [4, 3],
      [1, 5],
      [6, 5.5],
      [0, 0],
    ]);
    const marks = named("Belts").markPoint?.data ?? [];
    expect(marks.map((m) => m.label.formatter)).toEqual(["{badge|α}", "{badge|B β}", "{badge|A β}", "{badge|B 1}"]);
    expect(marks[0].coord).toEqual([4, 3]);
  });

  it("fills_the_scatter_badges_by_belt_and_red_for_unpaired_peaks", () => {
    const marks = named("Belts").markPoint?.data ?? [];
    expect(marks.map((m) => m.label.rich.badge.backgroundColor)).toEqual([T.muted, T.series[0], T.accent, T.danger]);
    expect(marks.map((m) => m.itemStyle?.color)).toEqual([T.text, T.series[0], T.accent, T.series[0]]);
  });

  it("splits_a_pair_that_is_apart_at_each_belts_own_peak", () => {
    const marks = named("Belts").markPoint?.data ?? [];
    expect(marks[1].coord).toEqual([1, 5]);
    expect(marks[2].coord).toEqual([6, 5.5]);
  });
});

describe("the belt charts' zoom", () => {
  type Zoom = {
    toolbox: { top: number; feature: Record<string, { yAxisIndex?: string; brushStyle: object }> };
    dataZoom: { type: string; xAxisIndex: number; yAxisIndex?: number }[];
  };
  const single = beltSingleOption(beltSpectrum(), "A (axis 1,-1)") as unknown as Zoom;

  it.each([
    ["overlay", beltOverlayOption(beltsResult()) as unknown as Zoom],
    ["single", single],
  ])("the_%s_zooms_the_x_axis_by_box_and_wheel", (_, option) => {
    expect(Object.keys(option.toolbox.feature)).toEqual(["dataZoom"]);
    expect(option.toolbox.feature.dataZoom.yAxisIndex).toBe("none");
    expect(option.toolbox.top).toBeLessThan(0);
    expect(option.toolbox.feature.dataZoom.brushStyle).toEqual({
      color: T.alpha(T.accent, 0.15),
      borderColor: T.accent,
      borderWidth: 1,
    });
    expect(option.dataZoom).toEqual([{ type: "inside", xAxisIndex: 0 }]);
  });

  it("the_scatter_zooms_both_axes", () => {
    const option = beltScatterOption(beltsResult()) as unknown as Zoom;
    expect(option.toolbox.feature.dataZoom.yAxisIndex).toBeUndefined();
    expect(option.dataZoom).toEqual([{ type: "inside", xAxisIndex: 0, yAxisIndex: 0 }]);
  });
});

describe("the belt chart curves", () => {
  const lineSeries = (option: unknown) =>
    (option as { series: { type: string; name: string; showSymbol?: boolean; symbol?: string }[] }).series.filter(
      (s) => s.type === "line",
    );

  it.each([
    ["overlay", beltOverlayOption(beltsResult())],
    ["single", beltSingleOption(beltSpectrum(), "A (axis 1,-1)")],
    ["scatter", beltScatterOption(beltsResult())],
  ])("the_%s_draws_every_line_without_point_symbols", (_, option) => {
    const lines = lineSeries(option);
    expect(lines.length).toBeGreaterThan(0);
    for (const line of lines) {
      expect(line.showSymbol).toBe(false);
      expect(line.symbol).toBe("none");
    }
  });

  it("the_scatter_names_its_axes_in_the_middle_so_they_stay_clear_of_the_legend", () => {
    const option = beltScatterOption(beltsResult()) as unknown as {
      xAxis: { name: string; nameLocation: string };
      yAxis: { name: string; nameLocation: string };
    };
    expect([option.xAxis.name, option.xAxis.nameLocation]).toEqual(["Belt A", "middle"]);
    expect([option.yAxis.name, option.yAxis.nameLocation]).toEqual(["Belt B", "middle"]);
  });
});

describe("the belt chart tooltips", () => {
  type Item = { seriesName: string; marker: string; value: number[] };
  type Tip = { trigger: string; formatter: (params: Item[]) => string };
  const tip = (option: unknown) => (option as { tooltip: Tip }).tooltip;

  it.each([
    ["overlay", beltOverlayOption(beltsResult())],
    ["single", beltSingleOption(beltSpectrum(), "A (axis 1,-1)")],
  ])("the_%s_tooltip_heads_with_the_frequency_and_names_each_series", (_, option) => {
    const text = tip(option).formatter([
      { seriesName: "Belt A", marker: "<i>a</i>", value: [12.5, 787.9369679595126] },
      { seriesName: "Belt B", marker: "<i>b</i>", value: [12.5, 4.555e6] },
    ]);
    expect(text).toBe("12.5 Hz<br/><i>a</i>Belt A: 7.879e+2<br/><i>b</i>Belt B: 4.555e+6");
  });

  it("the_overlay_tooltip_trims_the_frequency_to_two_decimals", () => {
    const text = tip(beltOverlayOption(beltsResult())).formatter([
      { seriesName: "Belt A", marker: "", value: [0.4, 1] },
    ]);
    expect(text.startsWith("0.4 Hz<br/>")).toBe(true);
  });

  it("the_scatter_tooltip_reads_belt_a_and_belt_b_from_the_belt_curve_only", () => {
    const option = beltScatterOption(beltsResult());
    expect(tip(option).formatter([{ seriesName: "Belts", marker: "", value: [787.9369679595126, 0.4] }])).toBe(
      "Belt A: 7.879e+2<br/>Belt B: 4.000e-1",
    );
    expect(tip(option).formatter([{ seriesName: "Ideal line", marker: "", value: [1, 1] }])).toBe("");
  });

  it("hides_the_good_zone_and_the_ideal_line_from_the_scatter_tooltip", () => {
    const list = (beltScatterOption(beltsResult()) as unknown as { series: { name: string; tooltip?: object }[] })
      .series;
    for (const name of ["Good zone floor", "Good zone", "Ideal line"]) {
      expect(list.find((s) => s.name === name)?.tooltip).toEqual({ show: false });
    }
  });
});
