import { parseNumber } from "@/lib/parse";

export const DEFAULT_MAX_FREQ = 200;
export const MIN_MAX_FREQ = 10;

/** The analysis maximum from typed text; `undefined` when it is not a finite number of at least 10. */
export function parseMaxFreq(text: string): number | undefined {
  const value = parseNumber(text);
  return value !== undefined && Number.isFinite(value) && value >= MIN_MAX_FREQ ? value : undefined;
}
