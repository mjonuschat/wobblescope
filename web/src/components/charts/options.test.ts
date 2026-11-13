import { describe, expect, it } from "vitest";

import { formatHz } from "@/lib/format";
import { chipColor } from "@/pages/stream/colors";
import { analysis, beltSpectrum, beltsResult, candidate, recommendation, spectrogram } from "@/test/fixtures";

import {
  beltOverlayOption,
  beltScatterOption,
  beltSingleOption,
  goodZoneBoundary,
  liveSpectrumOption,
  pointLabel,
  psdOption,
  spectrogramOption,
  summedOption,
} from "./options";

type Series = {
  name: string;
  yAxisIndex?: number;
  markLine?: { data: { xAxis?: number; yAxis?: number }[] };
  markArea?: { data: { yAxis: number }[][] };
  markPoint?: {
    data: {
      label: {
        formatter: string;
        color: string;
        fontWeight: string;
        fontSize: number;
        align: string;
        offset: [number, number];
        rich: Record<string, { backgroundColor?: string; color: string; width?: number; align?: string }>;
      };
    }[];
  };
};

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
    expect(marked.some((m) => m.yAxis !== undefined)).toBe(false);
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

  it("does not animate, since it is rebuilt several times a second", () => {
    expect((option as { animation?: boolean }).animation).toBe(false);
  });

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

describe("the frequency of interest on the live spectrum", () => {
  const spectrum = {
    freq_bins: [0, 10, 20],
    psd_x: [1, 2, 3],
    psd_y: [1, 2, 3],
    psd_z: [1, 2, 3],
    psd_sum: [3, 6, 9],
  };
  const chips = [{ chip: "a", spectrum, peaks: [], peakHold: [3, 6, 9], axes: ["sum" as const] }];

  it("marks it with a vertical line on the first series", () => {
    const option = liveSpectrumOption({ chips, maxHz: 200, frequencyOfInterest: 52 });
    const first = series(option)[0] as unknown as { markLine?: { data: { xAxis: number }[] } };
    expect(first.markLine?.data[0].xAxis).toBe(52);
  });

  it("has no marker without one", () => {
    const first = series(liveSpectrumOption({ chips, maxHz: 200 }))[0] as unknown as { markLine?: unknown };
    expect(first.markLine).toBeUndefined();
  });
});

describe("psdOption peaks, thresholds and scale", () => {
  const one = analysis().per_chip[0];
  const found = {
    peaks: [
      { freq: 5.2, level: 4, above_effect: false },
      { freq: 5.4, level: 5, above_effect: true },
    ],
    detect_threshold: 1,
    effect_threshold: 3,
  };
  /** The label starts at the peak, so it moves left by half the number badge (its width and padding). */
  const HALF_BADGE = -14;
  const withPeaks = { ...one, peaks: [[0, found]] as typeof one.peaks };
  const totals = (o: unknown) => series(o).filter((s) => s.name.endsWith("Total"));

  it("marks each peak on its dataset's total series with its number and its frequency", () => {
    const [total] = totals(psdOption({ chip: withPeaks, recommendation: null, maxFreq: 200 }));
    expect(total.markPoint?.data.map((d) => d.label.formatter)).toEqual([
      `{index|1} {value|${formatHz(5.2)} Hz}`,
      `{index|2} {value|${formatHz(5.4)} Hz}`,
    ]);
  });

  it("lifts the label of a peak that sits close to an earlier one, so labels do not overlap", () => {
    const [total] = totals(psdOption({ chip: withPeaks, recommendation: null, maxFreq: 200 }));
    expect(total.markPoint?.data.map((d) => d.label.offset)).toEqual([
      [HALF_BADGE, 0],
      [HALF_BADGE, -16],
    ]);
  });

  it("centres the number badge on the peak, with the frequency running to its right", () => {
    const [total] = totals(psdOption({ chip: withPeaks, recommendation: null, maxFreq: 200 }));
    for (const { label } of total.markPoint?.data ?? []) {
      expect(label.align).toBe("left");
      expect(label.rich.index.align).toBe("center");
      expect(label.rich.index.width).toBe(18);
      expect(label.offset[0]).toBe(HALF_BADGE);
    }
  });

  it("alternates the tiers along a run of close peaks and leaves distant peaks level", () => {
    const peak = (freq: number) => ({ freq, level: 4, above_effect: false });
    const run = { ...found, peaks: [20, 30, 40, 150].map(peak) };
    const chip = { ...one, peaks: [[0, run]] as typeof one.peaks };
    const [total] = totals(psdOption({ chip, recommendation: null, maxFreq: 200 }));
    expect(total.markPoint?.data.map((d) => d.label.offset[1])).toEqual([0, -16, 0, 0]);
  });

  it("draws the peak labels at 13 px", () => {
    const [total] = totals(psdOption({ chip: withPeaks, recommendation: null, maxFreq: 200 }));
    expect(total.markPoint?.data.map((d) => d.label.fontSize)).toEqual([13, 13]);
  });

  it("sets the number apart from the frequency as a badge", () => {
    const [total] = totals(psdOption({ chip: withPeaks, recommendation: null, maxFreq: 200 }));
    const [below, above] = total.markPoint?.data.map((d) => d.label) ?? [];
    for (const label of [below, above]) {
      expect(label.rich.index.backgroundColor).toBeTruthy();
      expect(label.rich.value.backgroundColor).toBeUndefined();
    }
    expect(above.rich.index.backgroundColor).not.toBe(below.rich.index.backgroundColor);
  });

  it("draws a peak above the effect threshold in bold and in a colour of its own", () => {
    const [total] = totals(psdOption({ chip: withPeaks, recommendation: null, maxFreq: 200 }));
    const [below, above] = total.markPoint?.data.map((d) => d.label) ?? [];
    expect(above.fontWeight).toBe("bold");
    expect(below.fontWeight).toBe("normal");
    expect(above.color).not.toBe(below.color);
  });

  it("draws both threshold lines and the relax and warning bands on the first dataset only", () => {
    const second = [1, one.datasets[0][1]] as (typeof one.datasets)[number];
    const chip = {
      ...one,
      datasets: [one.datasets[0], second],
      peaks: [
        [0, found],
        [1, found],
      ] as typeof one.peaks,
    };
    const [first, other] = totals(psdOption({ chip, recommendation: null, maxFreq: 200 }));
    expect(first.markLine?.data.map((m) => m.yAxis)).toEqual([1, 3]);
    expect(first.markArea?.data.map(([from, to]) => [from.yAxis, to.yAxis])).toEqual([
      [0, 1],
      [1, 3],
    ]);
    expect(other.markLine).toBeUndefined();
    expect(other.markArea).toBeUndefined();
    expect(other.markPoint?.data).toHaveLength(2);
  });

  it("keeps the recommended frequency line next to the threshold lines", () => {
    const o = psdOption({ chip: withPeaks, recommendation: recommendation(candidate("mzv", 48.2)), maxFreq: 200 });
    const lines = totals(o)[0].markLine?.data ?? [];
    expect(lines.map((m) => m.yAxis ?? m.xAxis)).toEqual([1, 3, 48.2]);
  });

  const leftAxis = (o: unknown) =>
    (o as { yAxis: { min: number; max: number; axisLabel: { formatter: (v: number) => string } }[] }).yAxis[0];

  it("writes the left axis labels to three significant digits", () => {
    const { formatter } = leftAxis(psdOption({ chip: one, recommendation: null, maxFreq: 200 })).axisLabel;
    expect(formatter(1341.6751077824945)).toBe("1,340");
    expect(formatter(300)).toBe("300");
    expect(formatter(0.0123456)).toBe("0.0123");
    expect(formatter(0)).toBe("0");
  });

  it("sets the left axis to 0 and the given maximum", () => {
    const axis = leftAxis(psdOption({ chip: one, recommendation: null, maxFreq: 200, maxScale: 10 }));
    expect([axis.min, axis.max]).toEqual([0, 10]);
  });

  it("without a maximum, ends the left axis at 1.05 times the largest total up to maxFreq", () => {
    for (const maxScale of [undefined, null, 0]) {
      const axis = leftAxis(psdOption({ chip: one, recommendation: null, maxFreq: 200, maxScale }));
      expect(axis.min).toBe(0);
      expect(axis.max).toBeCloseTo(5.25, 10);
    }
    const cut = leftAxis(psdOption({ chip: one, recommendation: null, maxFreq: 5.2 }));
    expect(cut.max).toBeCloseTo(4.2, 10);
  });
});

describe("spectrogramOption", () => {
  const map = spectrogram();
  const peaks = {
    peaks: [
      { freq: 1.4, level: 5, above_effect: false },
      { freq: 4.4, level: 9, above_effect: true },
      { freq: 3, level: 7, above_effect: false },
    ],
    detect_threshold: 1,
    effect_threshold: 3,
  };
  type Heat = {
    series: {
      data: number[][];
      markLine?: {
        data: {
          xAxis: number;
          label: {
            formatter: string;
            offset: [number, number];
            align: string;
            rich: { index: { width: number; align: string; padding: number[] } };
          };
        }[];
      };
    }[];
    grid: { top: number };
    visualMap: { min: number; max: number };
    xAxis: {
      type: string;
      data: string[];
      axisLabel: { interval: (i: number) => boolean; formatter: (v: string, i: number) => string };
    };
    yAxis: {
      type: string;
      name: string;
      data: string[];
      inverse?: boolean;
      axisLabel: { formatter: (v: string) => string };
    };
    tooltip: { formatter: (p: { data: number[] }) => string };
  };
  const option = (p: typeof peaks | null = null) => spectrogramOption(map, p, 200) as unknown as Heat;

  it("draws one heatmap cell per segment and bin", () => {
    const [heat] = option().series;
    expect(heat.data).toHaveLength(12);
    expect(heat.data[0]).toEqual([0, 0, Math.log10(1)]);
    expect(heat.data[1]).toEqual([1, 0, Math.log10(10)]);
    expect(heat.data[7]).toEqual([3, 1, Math.log10(500)]);
  });

  it("colours by log10 power from the floor to the maximum", () => {
    const { visualMap } = option();
    expect(visualMap.min).toBe(0);
    expect(visualMap.max).toBe(3);
  });

  it("draws a power below the floor in the colour of the floor", () => {
    const [heat] = option().series;
    // The row 2 cell of power 0 has the value of the floor, never a logarithm of zero.
    expect(heat.data[8]).toEqual([0, 2, 0]);
  });

  it("names the time axis Seconds", () => {
    expect(option().yAxis.name).toBe("Seconds");
  });

  it("puts frequency across and the earliest time at the bottom", () => {
    const { xAxis, yAxis } = option();
    expect(xAxis.data).toEqual(["0", "1.5", "3", "4.5"]);
    expect(yAxis.data).toEqual(["0.3", "0.6", "0.9"]);
    expect(yAxis.inverse).not.toBe(true);
  });

  it("labels frequency and time to one decimal", () => {
    const { xAxis, yAxis } = option();
    expect(yAxis.axisLabel.formatter("0.32")).toBe("0.3");
    expect(xAxis.data).toEqual(["0", "1.5", "3", "4.5"]);
  });

  it("ticks the frequency axis at multiples of 25 Hz, at the bins nearest to them", () => {
    const bins = Array.from({ length: 129 }, (_, i) => i * 1.5625);
    const wide = { ...map, freq_bins: bins, power: map.power.map(() => bins.map(() => 5)) };
    const { xAxis } = spectrogramOption(wide, null, 200) as unknown as Heat;
    const shown = [...bins.keys()].filter((i) => xAxis.axisLabel.interval(i));
    expect(shown).toEqual([0, 16, 32, 48, 64, 80, 96, 112, 128]);
    expect(shown.map((i) => xAxis.axisLabel.formatter(xAxis.data[i], i))).toEqual([
      "0",
      "25",
      "50",
      "75",
      "100",
      "125",
      "150",
      "175",
      "200",
    ]);
  });

  it("leaves a multiple of 25 Hz unticked when no bin lies near it", () => {
    // The fixture's bins stop at 4.5 Hz, so only 0 Hz has a tick.
    const { xAxis } = option();
    const shown = [0, 1, 2, 3].filter((i) => xAxis.axisLabel.interval(i));
    expect(shown).toEqual([0]);
  });

  it("draws the peaks as dotted vertical lines at the nearest bin", () => {
    const lines = option(peaks).series[0].markLine?.data.map((m) => m.xAxis);
    expect(lines).toEqual([1, 3, 2]);
    expect(option().series[0].markLine).toBeUndefined();
  });

  it("labels each peak line with its number and its frequency", () => {
    const labels = option(peaks).series[0].markLine?.data.map((m) => m.label.formatter);
    expect(labels).toEqual(["{index|1} {value|1.4 Hz}", "{index|2} {value|4.4 Hz}", "{index|3} {value|3.0 Hz}"]);
  });

  it("centres the number badge on the peak line, with the frequency running to its right", () => {
    const [first] = option(peaks).series[0].markLine?.data ?? [];
    const { align, offset, rich } = first.label;
    expect(align).toBe("left");
    expect(rich.index.align).toBe("center");
    // The label starts at the line, so it moves left by half the badge: its width and horizontal padding.
    expect(offset[0]).toBe(-(rich.index.width + rich.index.padding[1] * 2) / 2);
  });

  it("lifts the labels of peak lines that would overlap and leaves room above the plot for them", () => {
    const bins = Array.from({ length: 129 }, (_, i) => i * 1.5625);
    const wide = { ...map, freq_bins: bins, power: map.power.map(() => bins.map(() => 5)) };
    const close = {
      ...peaks,
      peaks: [59, 90, 100, 110, 150].map((freq) => ({ freq, level: 5, above_effect: false })),
    };
    const o = spectrogramOption(wide, close, 200) as unknown as Heat;
    expect(o.series[0].markLine?.data.map((m) => m.label.offset[1])).toEqual([0, 0, -16, -32, 0]);
    expect(o.grid.top).toBe(24 + 32);
    expect(option().grid.top).toBe(24);
  });

  it("shows time, frequency and power when hovered", () => {
    const text = option().tooltip.formatter({ data: [1, 2, Math.log10(5)] });
    expect(text).toContain("0.90 s");
    expect(text).toContain("1.5 Hz");
    expect(text).toContain("5.00e+0");
  });
});

type Mark = { coord: number[]; label: { formatter: string; color: string; fontWeight: string } };
type BeltSeries = {
  name: string;
  stack?: string;
  data: number[][];
  markPoint?: { data: Mark[] };
  markLine?: { data: { coord: number[] }[][] };
};

describe("beltOverlayOption", () => {
  it("the_overlay_has_two_curves_and_marks_pairs_with_greek_letters_and_unpaired_with_numbers", () => {
    const option = beltOverlayOption(beltsResult(), null);
    const [a, b] = (option as { series: BeltSeries[] }).series;
    expect([a.name, b.name]).toEqual(["Belt A", "Belt B"]);
    expect(a.data).toEqual([
      [0, 0],
      [50, 4],
      [100, 1],
      [150, 6],
      [200, 0],
    ]);
    expect(a.markPoint?.data.map((m) => m.label.formatter)).toEqual(["α1", "β1"]);
    expect(b.markPoint?.data.map((m) => m.label.formatter)).toEqual(["α2", "β2", "1"]);
    const unpaired = b.markPoint?.data[2];
    expect(unpaired?.coord).toEqual([150, 5.5]);
    expect(unpaired?.label.color).toBe("#ff6b6b");
    expect(unpaired?.label.fontWeight).toBe("bold");
    expect(a.markLine?.data).toEqual([
      [{ coord: [50, 4] }, { coord: [50, 3] }],
      [{ coord: [150, 6] }, { coord: [100, 5] }],
    ]);
    expect((option as { xAxis: { max: number } }).xAxis.max).toBe(200);
  });

  it("a_fixed_max_scale_ends_the_y_axis", () => {
    const y = (max: number | null) => (beltOverlayOption(beltsResult(), max) as { yAxis: { max: number } }).yAxis.max;
    expect(y(40)).toBe(40);
    expect(y(null)).toBeCloseTo(6.6);
  });
});

describe("beltSingleOption", () => {
  it("draws one curve without peak marks", () => {
    const option = beltSingleOption(beltSpectrum(), "A (axis 1,-1)", null);
    const list = (option as { series: BeltSeries[] }).series;
    expect(list).toHaveLength(1);
    expect(list[0].markPoint).toBeUndefined();
    expect(list[0].data).toHaveLength(5);
    expect((option as { yAxis: { max: number } }).yAxis.max).toBeCloseTo(6.6);
    expect((beltSingleOption(beltSpectrum(), "B (axis 1, 1)", 9) as { yAxis: { max: number } }).yAxis.max).toBe(9);
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
    const overlay = beltOverlayOption(beltsResult(), null) as unknown as typeof axes;
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
    expect(marks.map((m) => m.label.formatter)).toEqual(["α1/α2", "β2", "β1", "1"]);
    expect(marks[0].coord).toEqual([4, 3]);
  });
});
