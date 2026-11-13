import { describe, expect, it } from "vitest";

import type { TestState } from "@/lib/generated/TestState";

import { NO_TRACES, extend, levelOf, liveSignalOption } from "./trace";

const state = (over: Partial<TestState> = {}): TestState => ({
  id: "t",
  phase: "measuring",
  segment: 1,
  segments: 2,
  axis: "X",
  position: [150, 150, 20],
  expected_segment_seconds: 100,
  elapsed_seconds: 10,
  frequency_hz: 12,
  frequency_range: [10, 135],
  chips: [
    { chip: "a", level: [5, 50, 500], warnings: [] },
    { chip: "b", level: null, warnings: [] },
  ],
  ...over,
});

describe("levelOf", () => {
  it("takes the signal of the tested axis", () => {
    const chip = { chip: "a", level: [5, 50, 500] as [number, number, number], warnings: [] };
    expect(levelOf(chip, "X")).toBe(5);
    expect(levelOf(chip, "Y")).toBe(50);
    expect(levelOf({ ...chip, level: null }, "X")).toBeNull();
  });

  it("combines X and Y for a diagonal segment", () => {
    const chip = { chip: "a", level: [3, 4, 500] as [number, number, number], warnings: [] };
    expect(levelOf(chip, { Vector: [1, -1, 0] })).toBe(5);
  });
});

describe("extend", () => {
  it("adds a point at the reported frequency for each chip that has a signal", () => {
    const traces = extend(NO_TRACES, state());
    expect(traces.series.find((s) => s.chip === "a")?.points).toEqual([[12, 5]]);
    expect(traces.series.find((s) => s.chip === "b")?.points).toEqual([]);
  });

  it("adds nothing until Klipper has reported a frequency", () => {
    expect(extend(NO_TRACES, state({ frequency_hz: null })).series).toEqual([]);
  });

  it("keeps one point per Hz with the larger signal", () => {
    let traces = extend(NO_TRACES, state());
    traces = extend(traces, state({ chips: [{ chip: "a", level: [9, 1, 1], warnings: [] }] }));
    traces = extend(traces, state({ chips: [{ chip: "a", level: [2, 1, 1], warnings: [] }] }));
    expect(traces.series[0].points).toEqual([[12, 9]]);
    traces = extend(traces, state({ frequency_hz: 13 }));
    expect(traces.series[0].points).toEqual([
      [12, 9],
      [13, 5],
    ]);
  });

  it("returns the same traces when nothing changed", () => {
    const traces = extend(NO_TRACES, state());
    expect(extend(traces, state())).toBe(traces);
  });

  it("starts a new segment from an empty chart", () => {
    const first = extend(NO_TRACES, state());
    const second = extend(first, state({ segment: 2, frequency_hz: 5 }));
    expect(second.series[0].points).toEqual([[5, 5]]);
  });
});

describe("liveSignalOption", () => {
  it("draws a line per chip and a legend only for more than one", () => {
    const one = liveSignalOption([{ chip: "a", points: [[1, 2]] }], [10, 135]) as {
      series: unknown[];
      legend: unknown;
    };
    expect(one.series).toHaveLength(1);
    expect(one.legend).toBeUndefined();
    const two = liveSignalOption(
      [
        { chip: "a", points: [] },
        { chip: "b", points: [] },
      ],
      [10, 135],
    ) as { legend: unknown };
    expect(two.legend).toBeDefined();
  });

  it("fixes the frequency axis to the range of the run before any point arrives", () => {
    const option = liveSignalOption([], [10, 135]) as { xAxis: { min: number; max: number } };
    expect(option.xAxis).toMatchObject({ min: 10, max: 135 });
  });
});
