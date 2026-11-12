import { describe, expect, it } from "vitest";

import { SharedAutoScale, fixedScale } from "./spectrogramScale";

const flat = (level: number, n = 64) => Array.from({ length: n }, (_, i) => level + (i % 8) * 0.01);

describe("SharedAutoScale", () => {
  it("gives every chip the same scale", () => {
    const scale = new SharedAutoScale();
    const quiet = scale.columnScale("a", 0, flat(1));
    const loud = scale.columnScale("b", 0, flat(5));
    expect(scale.columnScale("a", 100, flat(1))).toEqual(scale.current());
    expect(loud).toEqual(scale.current());
    expect(quiet.low).toBeLessThan(2);
  });

  it("lets a loud chip move the scale for a quiet one", () => {
    const alone = new SharedAutoScale();
    for (let i = 0; i < 30; i++) alone.columnScale("a", i * 100, flat(1));
    const shared = new SharedAutoScale();
    for (let i = 0; i < 30; i++) {
      shared.columnScale("a", i * 100, flat(1));
      shared.columnScale("b", i * 100, flat(5));
    }
    expect(shared.current().range).toBeGreaterThan(alone.current().range * 10);
  });

  it("forgets what is older than a minute", () => {
    const scale = new SharedAutoScale();
    for (let i = 0; i < 12; i++) scale.columnScale("a", i * 100, flat(5));
    for (let i = 0; i < 12; i++) scale.columnScale("a", 120_000 + i * 100, flat(1));
    const { low, range } = scale.current();
    expect(low + range).toBeLessThan(2);
  });
});

describe("SharedAutoScale with a chip that stopped", () => {
  it("forgets a chip that has not reported for a minute", () => {
    const scale = new SharedAutoScale();
    for (let i = 0; i < 12; i++) scale.columnScale("loud", i * 100, flat(5));
    for (let i = 0; i < 12; i++) scale.columnScale("quiet", 120_000 + i * 100, flat(1));
    const { low, range } = scale.current();
    expect(low + range).toBeLessThan(2);
  });
});

describe("fixedScale", () => {
  it("is the same for every chip and moment, and takes powers, not logarithms", () => {
    const scale = fixedScale(100, 100_000);
    expect(scale.columnScale("a", 0, [1, 2])).toEqual({ low: 2, range: 3 });
    expect(scale.columnScale("b", 5_000, [9, 9])).toEqual(scale.current());
  });
});
