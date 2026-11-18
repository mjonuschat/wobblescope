import { describe, expect, it } from "vitest";

import { dark, light, resolve } from "@/test/tokens";

function linear(color: string): [number, number, number] {
  const m = /^oklch\(([\d.]+) ([\d.]+) ([\d.]+)\)$/.exec(color);
  if (!m) throw new Error(`not an opaque oklch color: ${color}`);
  const [L, C, h] = [Number(m[1]), Number(m[2]), (Number(m[3]) * Math.PI) / 180];
  const a = C * Math.cos(h);
  const b = C * Math.sin(h);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const mm = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const rgb = [
    4.0767416621 * l - 3.3077115913 * mm + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * mm - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * mm + 1.707614701 * s,
  ];
  return rgb.map((v) => Math.min(1, Math.max(0, v))) as [number, number, number];
}

function luminance(color: string): number {
  const [r, g, b] = linear(color);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

describe("contrast helper", () => {
  it("measures black on white as 21:1 and a mid grey on white below 4.5:1", () => {
    expect(contrast("oklch(1 0 0)", "oklch(0 0 0)")).toBeCloseTo(21, 0);
    expect(contrast("oklch(0.7 0 0)", "oklch(1 0 0)")).toBeLessThan(4.5);
  });
});

const SERIES = ["--series-1", "--series-2", "--series-3", "--series-4", "--series-5"];
const surfaces = ["--background", "--card", "--popover", "--sidebar"];
const statuses = ["--destructive", "--success", "--warning", "--info"];

const pairs: [string, string, number][] = [
  ...surfaces.map((s): [string, string, number] => ["--foreground", s, 4.5]),
  ...surfaces.map((s): [string, string, number] => ["--muted-foreground", s, 4.5]),
  ["--muted-foreground", "--muted", 4.5],
  ["--accent-foreground", "--accent", 4.5],
  ["--brand-foreground", "--brand", 4.5],
  ["--primary-foreground", "--primary", 4.5],
  ["--brand-ink", "--background", 4.5],
  ["--brand-ink", "--card", 4.5],
  ...SERIES.map((n): [string, string, number] => [n, "--card", 3]),
  ...statuses.flatMap((s): [string, string, number][] => [
    [s, "--background", 4.5],
    [s, "--card", 4.5],
    [`${s}-foreground`, s, 4.5],
  ]),
];

describe.each([
  ["light", light],
  ["dark", dark],
] as const)("%s theme contrast", (_, vars) => {
  it.each(pairs)("%s on %s reaches %s:1", (fg, bg, min) => {
    const ratio = contrast(resolve(vars, fg), resolve(vars, bg));
    expect(ratio, `${fg} on ${bg} is ${ratio.toFixed(2)}:1`).toBeGreaterThanOrEqual(min);
  });
});
