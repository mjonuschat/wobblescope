import { describe, expect, it } from "vitest";

import { iconNodeToPath, RESET_ZOOM_ICON, ZOOM_IN_ICON } from "./lucidePath";

describe("iconNodeToPath", () => {
  it("turns a circle into two arcs and a line into a move and a line", () => {
    const d = iconNodeToPath([
      ["circle", { cx: "11", cy: "11", r: "8" }],
      ["line", { x1: "21", x2: "16.65", y1: "21", y2: "16.65" }],
    ]);
    expect(d).toBe("M3 11a8 8 0 1 0 16 0a8 8 0 1 0 -16 0M21 21L16.65 16.65");
  });

  it("turns a polyline into a move and lines", () => {
    expect(iconNodeToPath([["polyline", { points: "1 2 3 4 5 6" }]])).toBe("M1 2L3 4L5 6");
  });

  it("rejects a shape it cannot convert", () => {
    expect(() => iconNodeToPath([["rect", {}]])).toThrow("rect");
  });
});

describe("the lucide toolbox icons", () => {
  it("are path:// strings built from the installed icons", () => {
    expect(ZOOM_IN_ICON).toMatch(/^path:\/\/M3 11a8 8 0 1 0 16 0.*M21 21L16\.65 16\.65/);
    expect(RESET_ZOOM_ICON).toMatch(/^path:\/\/M3 12a9 9/);
  });
});
