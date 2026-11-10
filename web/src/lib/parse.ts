/** A number typed into a field, `undefined` when it is empty and `NaN` when it is not a number. */
export function parseNumber(text: string): number | undefined {
  const trimmed = text.trim()
  if (trimmed === '') return undefined
  const value = Number(trimmed)
  return Number.isFinite(value) ? value : Number.NaN
}
