import type { FitParams } from "@/lib/generated/FitParams";
import { parseNumber } from "@/lib/parse";

/** Klipper's own fit: square corner velocity 5 mm/s and every other setting left to `core`. */
export const defaultFitParams: FitParams = {
  scv: 5,
  damping_ratio: null,
  max_smoothing: null,
  max_vibrations: null,
  test_damping_ratios: null,
  max_freq: null,
  shapers: null,
};

/** What the form fields hold as text. */
export interface FitDraft {
  scv: string;
  damping_ratio: string;
  max_smoothing: string;
  max_vibrations: string;
}

export const defaultDraft: FitDraft = {
  scv: String(defaultFitParams.scv),
  damping_ratio: "",
  max_smoothing: "",
  max_vibrations: "",
};

export type DraftResult = { params: FitParams } | { invalid: keyof FitDraft };

const number = (text: string): number | null => parseNumber(text) ?? null;

/** Turns the form text into fit parameters: an empty field is the default, text that is not a number is refused. */
export function draftToParams(draft: FitDraft): DraftResult {
  const parsed = {
    scv: number(draft.scv),
    damping_ratio: number(draft.damping_ratio),
    max_smoothing: number(draft.max_smoothing),
    max_vibrations: number(draft.max_vibrations),
  };
  for (const key of Object.keys(parsed) as (keyof FitDraft)[]) {
    if (Number.isNaN(parsed[key])) return { invalid: key };
  }
  return {
    params: {
      ...defaultFitParams,
      ...parsed,
      scv: parsed.scv ?? defaultFitParams.scv,
    },
  };
}
