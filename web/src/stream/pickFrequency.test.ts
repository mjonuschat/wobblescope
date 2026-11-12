import { describe, expect, it } from "vitest";

import { pickFrequency } from "./pickFrequency";

describe("pickFrequency", () => {
  it("snaps to a labelled peak within 3 Hz", () => {
    expect(pickFrequency(51.2, [52.4, 90])).toBe(52.4);
  });

  it("rounds a click elsewhere to 0.1 Hz", () => {
    expect(pickFrequency(70.04, [52.4, 90])).toBe(70);
    expect(pickFrequency(70.06, [])).toBe(70.1);
  });
});
