export const clampIndex = (index: number, length: number) => Math.max(0, Math.min(index, length - 1));

/** The index of the value in `values` closest to `target`; 0 for an empty list. */
export function closestIndex(values: number[], target: number): number {
  let best = 0;
  values.forEach((value, i) => {
    if (Math.abs(value - target) < Math.abs(values[best] - target)) best = i;
  });
  return best;
}

/** The index of the first value in the ascending `values` that is at least `target`, or the last index when none is. */
export function firstAtOrAbove(values: number[], target: number): number {
  const found = values.findIndex((v) => v >= target);
  return clampIndex(found === -1 ? values.length - 1 : found, values.length);
}
