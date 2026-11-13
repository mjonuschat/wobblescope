import type { Analysis } from "@/lib/generated/Analysis";
import type { DatasetDecision } from "@/lib/generated/DatasetDecision";
import type { Decision } from "@/lib/generated/Decision";
import type { CalibrationResult } from "@/lib/generated/CalibrationResult";
import type { FitParams } from "@/lib/generated/FitParams";
import type { FitSettings } from "@/lib/generated/FitSettings";
import type { Recommendation } from "@/lib/generated/Recommendation";
import type { Spectrogram } from "@/lib/generated/Spectrogram";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

export const defaultParams: FitParams = {
  scv: 5,
  damping_ratio: null,
  max_smoothing: null,
  max_vibrations: null,
  test_damping_ratios: null,
  max_freq: null,
  shapers: null,
};

/** Segment 0 is X (chips 0 and 1), segment 1 is Y (chip 0). */
export function twoAxisMeta(id = "s1"): SessionMeta {
  const file = (chip: number, segment: number) => ({
    chip,
    sample_file: `c${chip}-s${segment}.zst`,
    sample_count: 10,
    coverage: "Complete" as const,
    health: null,
    health_notes: [],
  });
  const chip = (name: string) => ({
    name,
    chip_type: null,
    nominal_rate_hz: null,
    observed_rate_hz: null,
    axes_map: null,
    mounting: null,
    health: "Ok" as const,
    health_notes: [],
  });
  return {
    format_version: 1,
    id,
    created_at: "2026-10-31T12:00:00Z",
    source: "Capture",
    time_domain: "KlipperPrintTime",
    mode: null,
    settings: null,
    printer: null,
    outcome: "Complete",
    notes: [],
    probe_points: [{ position: null }],
    chips: [chip("adxl345"), chip("lis2dw ebb36")],
    segments: [
      { axis: "X", point: 0, window: null, files: [file(0, 0), file(1, 0)] },
      { axis: "Y", point: 0, window: null, files: [file(0, 1)] },
    ],
  } as SessionMeta;
}

export const settings: FitSettings = {
  scv: 5,
  damping_ratio: 0.1,
  damping_source: "Fixed",
  resonance_freq: null,
  test_damping_ratios: [0.075, 0.1, 0.15],
  max_smoothing: null,
  max_vibrations: null,
  max_freq: 200,
  freq_step: 0.2,
  shapers: ["zv", "mzv", "ei", "2hump_ei", "3hump_ei"],
};

export function candidate(name: string, freq: number, over: Partial<CalibrationResult> = {}) {
  return {
    name,
    freq,
    vals: [0.9, 0.5, 0.1],
    vibrs: 0.032,
    dataset_vibrs: [0.032],
    smoothing: 0.12,
    score: 0.2,
    max_accel: 4500,
    ...over,
  } satisfies CalibrationResult;
}

export function recommendation(best = candidate("mzv", 48.2)): Recommendation {
  return {
    freq_bins: [5, 5.2, 5.4],
    best,
    for_performance: null,
    all: [candidate("zv", 55), best],
    settings,
    core_version: "0.1.0",
  };
}

/** One eligibility decision for point 0; `dropped` is set for a recording that could not be analysed. */
export function decision(
  chip: string,
  decision: Decision,
  reasons: string[] = [],
  dropped: string | null = null,
): DatasetDecision {
  return { chip, point: 0, decision, reasons, dropped };
}

/** Chip `a` has a recommendation, chip `b` has none. */
export function analysis(): Analysis {
  const rec = recommendation();
  const data = {
    name: "a#0",
    freq_bins: [5, 5.2, 5.4],
    psd_x: [1, 2, 3],
    psd_y: [1, 1, 1],
    psd_z: [1, 1, 1],
    psd_sum: [3, 4, 5],
    normalized: true,
  };
  return {
    per_chip: [
      { chip: "a", health: "Ok", datasets: [[0, data]], windows: [], peaks: [], recommendation: rec },
      { chip: "b", health: "Degraded", datasets: [[0, data]], windows: [], peaks: [], recommendation: null },
    ],
    worst_case: rec,
    worst_case_vibrations: [{ chip: "a", point: 0, vibrs: 0.032 }],
    summed: { freq_bins: [5, 5.2, 5.4], psd: [3, 4, 5], psd_x: [2, 3, 4], psd_y: [1, 1, 1], psd_z: [0, 0, 0] },
    failures: [],
    eligibility: { decisions: [] },
    core_version: "0.1.0",
  };
}

/** Three segments by four bins, from a floor of 1 to a maximum of 1000, with one cell of 0. */
export function spectrogram(): Spectrogram {
  return {
    freq_bins: [0, 1.5, 3, 4.5],
    times: [0.3, 0.6, 0.9],
    power: [
      [1, 10, 100, 1000],
      [2, 20, 200, 500],
      [0, 5, 50, 100],
    ],
    floor: 1,
    max: 1000,
  };
}
