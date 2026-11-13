import { describe, expect, it } from "vitest";

import { formatFrequency, formatPsd } from "./format";

describe("formatFrequency", () => {
  it.each([
    [0.4, "0.4"],
    [12.5, "12.5"],
    [100, "100"],
    [787.9369679595126, "787.94"],
    [0, "0"],
    [-0, "0"],
    [-0.001, "0"],
    [0.004, "0"],
  ])("writes %s as %s", (value, expected) => {
    expect(formatFrequency(value)).toBe(expected);
  });

  it("writes nothing for a value that is not finite", () => {
    expect(formatFrequency(Number.NaN)).toBe("");
  });
});

describe("formatPsd", () => {
  it.each([
    [787.9369679595126, "7.879e+2"],
    [4.555e6, "4.555e+6"],
    [0.4, "4.000e-1"],
    [100, "1.000e+2"],
    [1.23456e-9, "1.235e-9"],
    [-12.5, "-1.250e+1"],
    [0, "0"],
    [-0, "0"],
  ])("writes %s as %s", (value, expected) => {
    expect(formatPsd(value)).toBe(expected);
  });

  it("writes nothing for a value that is not finite", () => {
    expect(formatPsd(Number.POSITIVE_INFINITY)).toBe("");
  });
});
