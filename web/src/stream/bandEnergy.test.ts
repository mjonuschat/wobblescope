import { describe, expect, it } from "vitest";

import type { Spectrum } from "@/lib/generated/Spectrum";

import { BandHistory, bandEnergy, pickFrequency } from "./bandEnergy";

function flat(level: number, top = 10): Spectrum {
  const bins = Array.from({ length: top + 1 }, (_, i) => i);
  const fill = bins.map(() => level);
  return { freq_bins: bins, psd_x: fill, psd_y: fill, psd_z: fill, psd_sum: fill };
}

describe("bandEnergy", () => {
  it("sums the bins in the band times the bin spacing", () => {
    expect(bandEnergy(flat(2), 5, 2)).toBeCloseTo(10);
  });

  it("counts only the bins inside the spectrum at the edge", () => {
    expect(bandEnergy(flat(2), 0, 2)).toBeCloseTo(6);
  });

  it("is zero for an empty spectrum", () => {
    const empty: Spectrum = { freq_bins: [], psd_x: [], psd_y: [], psd_z: [], psd_sum: [] };
    expect(bandEnergy(empty, 5, 2)).toBe(0);
  });

  it("scales with the bin spacing", () => {
    const spectrum = flat(1);
    spectrum.freq_bins = spectrum.freq_bins.map((f) => f * 0.5);
    expect(bandEnergy(spectrum, 2.5, 1)).toBeCloseTo(2.5);
  });
});

describe("BandHistory", () => {
  it("keeps the last minute and keeps chips apart", () => {
    const history = new BandHistory();
    history.push("a", 0, 1);
    history.push("a", 30, 2);
    history.push("a", 61, 3);
    history.push("b", 5, 9);
    expect(history.series("a")).toEqual([
      { t: 30, v: 2 },
      { t: 61, v: 3 },
    ]);
    expect(history.series("b")).toEqual([{ t: 5, v: 9 }]);
    expect(history.series("c")).toEqual([]);
  });

  it("forgets everything on reset", () => {
    const history = new BandHistory();
    history.push("a", 0, 1);
    history.reset();
    expect(history.series("a")).toEqual([]);
  });
});

describe("pickFrequency", () => {
  it("snaps to a labelled peak within 3 Hz", () => {
    expect(pickFrequency(51.2, [52.4, 90])).toBe(52.4);
  });

  it("rounds a click elsewhere to 0.1 Hz", () => {
    expect(pickFrequency(70.04, [52.4, 90])).toBe(70);
    expect(pickFrequency(70.06, [])).toBe(70.1);
  });
});
