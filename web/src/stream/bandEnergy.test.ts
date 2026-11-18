import { describe, expect, it } from "vitest";

import type { Spectrum } from "@/lib/generated/Spectrum";

import { bandRms } from "./bandEnergy";

const spectrum = (psd: number[]): Spectrum => ({
  freq_bins: psd.map((_, i) => i * 10),
  psd_x: psd,
  psd_y: psd,
  psd_z: psd,
  psd_sum: psd,
});

describe("bandRms", () => {
  it("is the square root of the PSD integrated over the bins within the half width", () => {
    // Bins at 0, 10, 20 and 30 Hz, 10 Hz apart: the band 20 ± 10 holds three of them.
    expect(bandRms(spectrum([4, 4, 4, 4]), 20, 10)).toBeCloseTo(Math.sqrt(4 * 10 * 3), 9);
  });

  it("counts a bin on the edge of the band and none beyond it", () => {
    expect(bandRms(spectrum([0, 9, 0, 0]), 15, 5)).toBeCloseTo(Math.sqrt(9 * 10), 9);
    expect(bandRms(spectrum([0, 9, 0, 0]), 15, 4.9)).toBe(0);
  });

  it("is zero when no bin is in the band, and for a spectrum too short to have a bin width", () => {
    expect(bandRms(spectrum([1, 1, 1]), 100, 2)).toBe(0);
    expect(bandRms({ ...spectrum([1]), freq_bins: [5] }, 5, 2)).toBe(0);
  });

  it("grows with the vibration at the frequency, so a touched part shows", () => {
    expect(bandRms(spectrum([0, 0, 100, 0]), 20, 2)).toBeGreaterThan(bandRms(spectrum([0, 0, 1, 0]), 20, 2));
  });
});
