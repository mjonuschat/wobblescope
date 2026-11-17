import { describe, expect, it } from "vitest";

import { formatMm, parseNumber } from "./parse";

describe("parseNumber", () => {
  it("is undefined for an empty field, a number for a number and NaN for anything else", () => {
    expect(parseNumber("  ")).toBeUndefined();
    expect(parseNumber(" 2.5 ")).toBe(2.5);
    expect(parseNumber("abc")).toBeNaN();
    expect(parseNumber("Infinity")).toBeNaN();
  });
});

describe("formatMm", () => {
  it("drops float noise and trailing zeros", () => {
    expect(formatMm((0.1 + 300.3) / 2 - 15)).toBe("135.2");
    expect(formatMm(150.5)).toBe("150.5");
    expect(formatMm(20)).toBe("20");
    expect(formatMm(-2)).toBe("-2");
  });
});
