import type { Analysis } from "@/lib/generated/Analysis";
import type { Axis } from "@/lib/generated/Axis";
import type { FitParams } from "@/lib/generated/FitParams";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import { parseNumber } from "@/lib/parse";

/** Klipper's own fit: square corner velocity 5 mm/s, damping ratio 0.1 and every other setting left to `core`. */
export const defaultFitParams: FitParams = {
  scv: 5,
  damping_ratio: 0.1,
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

/** A fraction as a percentage, without the float noise of multiplying by 100. */
const toPercent = (fraction: number) => Number((fraction * 100).toPrecision(12));

/**
 * The form text for `params`: an unset value is an empty field. The vibration limit is a
 * fraction in the analysis and a percentage on the page, as every other vibration is.
 */
export function draftFrom(params: FitParams): FitDraft {
  const text = (value: number | null) => (value === null ? "" : String(value));
  return {
    scv: String(params.scv),
    damping_ratio: text(params.damping_ratio),
    max_smoothing: text(params.max_smoothing),
    max_vibrations: params.max_vibrations === null ? "" : String(toPercent(params.max_vibrations)),
  };
}

export const defaultDraft: FitDraft = draftFrom(defaultFitParams);

/** The fit a session starts from, and which of its values came from the printer. */
export interface FitBaseline {
  params: FitParams;
  recorded: { scv: boolean; damping: boolean };
}

/**
 * The fit to start from for `axis`: the printer's own square corner velocity and damping ratio
 * when the session recorded them, Klipper's defaults otherwise.
 */
export function baselineFor(meta: SessionMeta, axis: Axis): FitBaseline {
  const printer = meta.printer;
  const scv = printer?.square_corner_velocity ?? null;
  const damping = (axis === "X" ? printer?.damping_ratio_x : printer?.damping_ratio_y) ?? null;
  return {
    params: {
      ...defaultFitParams,
      scv: scv ?? defaultFitParams.scv,
      damping_ratio: damping ?? defaultFitParams.damping_ratio,
    },
    recorded: { scv: scv !== null, damping: damping !== null },
  };
}

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
      max_vibrations: parsed.max_vibrations === null ? null : Number((parsed.max_vibrations / 100).toPrecision(12)),
    },
  };
}

/** The fit settings an analysis used, from its recommendation. */
export function effectiveSettings(analysis: Analysis) {
  return (
    analysis.worst_case?.settings ?? analysis.per_chip.find((c) => c.recommendation)?.recommendation?.settings ?? null
  );
}
