import { describe, expect, it } from "vitest";

import { printerSettings } from "@/test/capture";

import { patternPoints } from "./planning";

const limits = { x: { min: 0, max: 300 }, y: { min: 0, max: 200 }, z: { min: 0, max: 300 } };

describe("patternPoints", () => {
  it("puts one point in the middle", () => {
    expect(patternPoints("center", limits, 20)).toEqual([[150, 100, 20]]);
  });

  it("adds the four corners, 20 mm inside the limits, to the center", () => {
    expect(patternPoints("five", limits, 20)).toEqual([
      [150, 100, 20],
      [20, 20, 20],
      [280, 20, 20],
      [280, 180, 20],
      [20, 180, 20],
    ]);
  });

  it("makes a 3 by 3 grid that snakes through the rows", () => {
    const points = patternPoints("nine", limits, 20);
    expect(points).toHaveLength(9);
    expect(points[0]).toEqual([20, 20, 20]);
    expect(points[3]).toEqual([280, 100, 20]);
    expect(points[8]).toEqual([280, 180, 20]);
  });

  it("keeps a smaller margin on a bed too small for 20 mm", () => {
    const small = { x: { min: 0, max: 60 }, y: { min: 0, max: 60 }, z: { min: 0, max: 10 } };
    const points = patternPoints("nine", small, 5);
    expect(points[0]).toEqual([15, 15, 5]);
    expect(points[8]).toEqual([45, 45, 5]);
  });

  it("works from the test printer's travel", () => {
    const { axis_limits } = printerSettings();
    expect(patternPoints("center", axis_limits as NonNullable<typeof axis_limits>, 20)[0].slice(0, 2)).toEqual([
      150.5, 154,
    ]);
  });
});
