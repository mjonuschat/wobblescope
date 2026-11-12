const SNAP_HZ = 3;

/**
 * The frequency a click on the spectrum picks: the nearest labelled peak within 3 Hz, else the
 * clicked frequency rounded to 0.1 Hz.
 */
export function pickFrequency(clicked: number, peaks: number[]): number {
  let nearest: number | null = null;
  for (const peak of peaks) {
    if (
      Math.abs(peak - clicked) <= SNAP_HZ &&
      (nearest === null || Math.abs(peak - clicked) < Math.abs(nearest - clicked))
    ) {
      nearest = peak;
    }
  }
  return nearest ?? Math.round(clicked * 10) / 10;
}
