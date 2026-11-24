import type { Analysis } from "@/lib/generated/Analysis";
import type { Axis } from "@/lib/generated/Axis";
import type { FitParams } from "@/lib/generated/FitParams";
import type { Recommendation } from "@/lib/generated/Recommendation";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import { parseNumber } from "@/lib/parse";

export const defaultFitParams: FitParams = {
  scv: 5,
  damping_ratio: null,
  max_smoothing: null,
  max_vibrations: null,
  test_damping_ratios: null,
  max_freq: null,
  max_shaper_freq: null,
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

export interface FitBaseline {
  params: FitParams;
  /** What the printer recorded. The damping ratio is only named; the fit estimates it unless one is entered. */
  recorded: {
    scv: boolean;
    damping: number | null;
    dampingByAxis?: Partial<Record<Axis, number>>;
  };
}

export function baselineFor(meta: SessionMeta, axis: Axis): FitBaseline {
  const printer = meta.printer;
  const scv = printer?.square_corner_velocity ?? null;
  const damping = (axis === "X" ? printer?.damping_ratio_x : printer?.damping_ratio_y) ?? null;
  return {
    params: {
      ...defaultFitParams,
      scv: scv ?? defaultFitParams.scv,
    },
    recorded: { scv: scv !== null, damping },
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

export function effectiveSettings(analysis: Analysis) {
  return (
    analysis.worst_case?.settings ?? analysis.per_chip.find((c) => c.recommendation)?.recommendation?.settings ?? null
  );
}

const DEFAULT_SHAPERS = ["zv", "mzv", "ei", "2hump_ei", "3hump_ei"];
const DEFAULT_FREQ_STEP = 0.2;

/**
 * The settings a fit used, for the recommendation card. The damping ratio is left out because the
 * card shows it beside the result. Mirrors `FitSettings::summary` in `core`: the limits, the step
 * and the shaper list appear only when set or not the default.
 */
export function fitSettingFields(settings: Recommendation["settings"]): { label: string; value: string }[] {
  const fields = [
    { label: "Corner velocity", value: `${settings.scv} mm/s` },
    { label: "Max frequency", value: `${settings.max_freq} Hz` },
  ];
  if (settings.max_smoothing !== null) {
    fields.push({ label: "Max smoothing", value: `${settings.max_smoothing} mm` });
  }
  if (settings.max_vibrations !== null) {
    fields.push({ label: "Max vibrations", value: `${toPercent(settings.max_vibrations)}%` });
  }
  if (settings.freq_step !== DEFAULT_FREQ_STEP) {
    fields.push({ label: "Frequency step", value: `${settings.freq_step} Hz` });
  }
  if (settings.shapers.join() !== DEFAULT_SHAPERS.join()) {
    fields.push({ label: "Shapers", value: settings.shapers.join(" ") });
  }
  return fields;
}

export function baselineForAxes(meta: SessionMeta, axes: Axis[]): FitBaseline {
  const first = baselineFor(meta, axes[0] ?? "X");
  if (axes.length < 2) return first;
  const dampingByAxis: Partial<Record<Axis, number>> = {};
  for (const axis of axes) {
    const damping = baselineFor(meta, axis).recorded.damping;
    if (damping !== null) dampingByAxis[axis] = damping;
  }
  return { ...first, recorded: { ...first.recorded, dampingByAxis } };
}
