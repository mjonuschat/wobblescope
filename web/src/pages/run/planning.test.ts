import { describe, expect, it } from "vitest";

import { printerSettings } from "@/test/capture";

import { formatPosition, patternPoints, segmentLabel } from "./planning";

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

describe("segmentLabel", () => {
  it("names the belts and the axes in title case", () => {
    expect(segmentLabel({ Vector: [1, -1, 0] })).toBe("Belt A (Axis 1,-1)");
    expect(segmentLabel({ Vector: [1, 1, 0] })).toBe("Belt B (Axis 1,1)");
    expect(segmentLabel("X")).toBe("X Axis");
    expect(segmentLabel("Y")).toBe("Y Axis");
  });

  it("keeps the vector's own text for any other vector", () => {
    expect(segmentLabel({ Vector: [1, 2, 0] })).toBe("1,2,0");
  });
});

describe("formatPosition", () => {
  it("labels each coordinate and adds the unit", () => {
    expect(formatPosition([75, 75.12345, 20])).toBe("X 75, Y 75.123, Z 20 mm");
  });
});
