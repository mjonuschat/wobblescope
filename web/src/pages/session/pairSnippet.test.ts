import { describe, expect, it } from "vitest";

import { pairSnippet } from "./pairSnippet";

const x = "[input_shaper]\n# Wobblescope: all chips, axis X\nshaper_type_x: mzv\nshaper_freq_x: 49.8\n";
const y = "[input_shaper]\n# Wobblescope: all chips, axis Y\nshaper_type_y: ei\nshaper_freq_y: 44.0\n";

describe("pairSnippet", () => {
  it("joins the x block and the y block under one section line", () => {
    const joined = pairSnippet(x, y) ?? "";
    expect(joined.split("[input_shaper]").length).toBe(2);
    expect(joined.startsWith(x)).toBe(true);
    expect(joined.endsWith("# Wobblescope: all chips, axis Y\nshaper_type_y: ei\nshaper_freq_y: 44.0\n")).toBe(true);
  });

  it("is null unless both axes have a snippet", () => {
    expect(pairSnippet(x, null)).toBeNull();
    expect(pairSnippet(null, y)).toBeNull();
  });

  it("keeps a y block that has no section line", () => {
    expect(pairSnippet(x, "shaper_type_y: ei\n")).toBe(`${x}shaper_type_y: ei\n`);
  });
});
