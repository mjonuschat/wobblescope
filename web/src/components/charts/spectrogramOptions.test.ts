import { describe, expect, it } from "vitest";

import { spectrogram } from "@/test/fixtures";

import { spectrogramOption } from "./spectrogramOptions";

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
