/** Number formatters for the belt chart tooltips. */

/** A frequency with at most two decimals and no trailing zeros: 0.4, 12.5, 100. */
export function formatFrequency(hz: number): string {
  if (!Number.isFinite(hz)) return "";
  // `+ 0` turns a negative zero, which rounding a tiny negative can produce, into 0.
  return String(Number(hz.toFixed(2)) + 0);
}

/** A value to four significant digits in the scientific style of the y axis labels: 7.879e+2. */
export function formatPsd(value: number): string {
  if (!Number.isFinite(value)) return "";
  if (value === 0) return "0";
  return value.toExponential(3);
}
