import { describe, expect, it } from "vitest";

import type { AxesMapResult } from "@/lib/generated/AxesMapResult";

import { formatMap, noiseText } from "./axesMapFormat";

describe("formatMap", () => {
  it("writes the same map one way whatever its spacing", () => {
    expect(formatMap("x,y,z")).toBe("x, y, z");
    expect(formatMap("x, -z, y")).toBe("x, -z, y");
    expect(formatMap(" x ,-y,  -z ")).toBe("x, -y, -z");
  });
});

describe("noiseText", () => {
  it("gives the level and the verdict in words", () => {
    const noise = (level: number, verdict: AxesMapResult["noise_verdict"]) =>
      noiseText({ noise: level, noise_verdict: verdict } as AxesMapResult);
    expect(noise(412.4, "High")).toBe("412 mm/s² (High)");
    expect(noise(120, "Fine")).toBe("120 mm/s² (OK)");
    expect(noise(900, "TooHigh")).toBe("900 mm/s² (Too high)");
  });
});
