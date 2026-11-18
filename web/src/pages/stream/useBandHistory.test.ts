import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { Spectrum } from "@/lib/generated/Spectrum";

import { BAND_SPAN_MS, useBandHistory } from "./useBandHistory";

const spectrum = (level: number): Spectrum => ({
  freq_bins: [0, 10, 20],
  psd_x: [0, level, 0],
  psd_y: [0, level, 0],
  psd_z: [0, level, 0],
  psd_sum: [0, level, 0],
});

describe("useBandHistory", () => {
  it("adds one point per new spectrum, measured around the frequency of interest", () => {
    let clock = 1000;
    const a1 = spectrum(4);
    const { result, rerender } = renderHook((props) => useBandHistory(props.spectra, props.center, 2, () => clock), {
      initialProps: { spectra: { a: a1 }, center: 10 as number | null },
    });
    expect(result.current.a).toEqual([{ at: 1000, rms: Math.sqrt(4 * 10) }]);
    // The same spectrum again, as when the page renders for another reason, adds nothing.
    clock = 1100;
    rerender({ spectra: { a: a1 }, center: 10 });
    expect(result.current.a).toHaveLength(1);
    clock = 1200;
    rerender({ spectra: { a: spectrum(16) }, center: 10 });
    expect(result.current.a.map((p) => p.rms)).toEqual([Math.sqrt(40), Math.sqrt(160)]);
  });

  it("records nothing while there is no frequency of interest", () => {
    const { result } = renderHook(() => useBandHistory({ a: spectrum(4) }, null, 2, () => 0));
    expect(result.current.a).toEqual([]);
  });

  it("drops points older than the span and forgets a chip that is no longer shown", () => {
    let clock = 0;
    const { result, rerender } = renderHook((props) => useBandHistory(props.spectra, 10, 2, () => clock), {
      initialProps: { spectra: { a: spectrum(1), b: spectrum(1) } as Record<string, Spectrum> },
    });
    clock = BAND_SPAN_MS + 1;
    rerender({ spectra: { a: spectrum(2), b: spectrum(2) } });
    expect(result.current.a).toHaveLength(1);
    expect(result.current.a[0].at).toBe(BAND_SPAN_MS + 1);
    rerender({ spectra: { a: spectrum(3) } });
    expect(Object.keys(result.current)).toEqual(["a"]);
  });
});
