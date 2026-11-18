import type { Spectrum } from "@/lib/generated/Spectrum";

/**
 * The RMS vibration, in the stream's acceleration unit, of the summed spectrum within
 * `halfWidthHz` of `centerHz`: the square root of the PSD integrated over those bins. Zero when no
 * bin falls in the band.
 */
export function bandRms(spectrum: Spectrum, centerHz: number, halfWidthHz: number): number {
  const { freq_bins: bins, psd_sum: psd } = spectrum;
  if (bins.length < 2) return 0;
  const spacing = bins[1] - bins[0];
  let power = 0;
  for (let i = 0; i < bins.length; i++) {
    if (Math.abs(bins[i] - centerHz) <= halfWidthHz) power += psd[i] * spacing;
  }
  return Math.sqrt(power);
}
