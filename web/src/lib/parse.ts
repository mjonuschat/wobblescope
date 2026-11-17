/** A number typed into a field, `undefined` when it is empty and `NaN` when it is not a number. */
export function parseNumber(text: string): number | undefined {
  const trimmed = text.trim();
  if (trimmed === "") return undefined;
  const value = Number(trimmed);
  return Number.isFinite(value) ? value : Number.NaN;
}

/** A length in mm as shown in a field: rounded to 0.01 so float noise from a midpoint does not show. */
export function formatMm(value: number): string {
  return String(Math.round(value * 100) / 100);
}
