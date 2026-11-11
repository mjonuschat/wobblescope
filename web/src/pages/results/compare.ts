import type { Analysis } from "@/lib/generated/Analysis";
import type { CalibrationResult } from "@/lib/generated/CalibrationResult";
import type { Recommendation } from "@/lib/generated/Recommendation";

/** Relative tolerance for numbers. The same core on the same data should match far closer. */
const TOLERANCE = 1e-6;

function close(a: number, b: number): boolean {
  return Math.abs(a - b) <= TOLERANCE * Math.max(Math.abs(a), Math.abs(b));
}

function show(value: string | number | number[]): string {
  return Array.isArray(value) ? `[${value.join(", ")}]` : String(value);
}

function differentFields(saved: CalibrationResult, fresh: CalibrationResult): string[] {
  const out: string[] = [];
  if (saved.name !== fresh.name) out.push(`name ${saved.name} -> ${fresh.name}`);
  const numbers = ["freq", "vibrs", "smoothing", "score", "max_accel"] as const;
  for (const field of numbers) {
    if (!close(saved[field], fresh[field])) out.push(`${field} ${saved[field]} -> ${fresh[field]}`);
  }
  const same =
    saved.dataset_vibrs.length === fresh.dataset_vibrs.length &&
    saved.dataset_vibrs.every((v, i) => close(v, fresh.dataset_vibrs[i]));
  if (!same) {
    out.push(`dataset_vibrs ${show(saved.dataset_vibrs)} -> ${show(fresh.dataset_vibrs)}`);
  }
  return out;
}

function compareRecommendation(label: string, saved: Recommendation | null, fresh: Recommendation | null): string[] {
  if (saved === null && fresh === null) return [];
  if (saved === null) return [`${label}: recommendation gained`];
  if (fresh === null) return [`${label}: recommendation lost`];
  return differentFields(saved.best, fresh.best).map((d) => `${label}: ${d}`);
}

/**
 * Compares what a result saved with a fresh analysis of the same samples. It checks the
 * worst case's `best` and each chip's `best`, and does not compare spectra or the full
 * candidate lists.
 */
export function compareAnalyses(saved: Analysis, fresh: Analysis): { sameAnswer: boolean; differences: string[] } {
  const differences = compareRecommendation("worst case", saved.worst_case, fresh.worst_case);
  const freshChips = new Map(fresh.per_chip.map((c) => [c.chip, c]));
  for (const chip of saved.per_chip) {
    const other = freshChips.get(chip.chip);
    if (!other) {
      differences.push(`chip ${chip.chip}: missing from the re-analysis`);
      continue;
    }
    differences.push(...compareRecommendation(`chip ${chip.chip}`, chip.recommendation, other.recommendation));
  }
  const savedNames = new Set(saved.per_chip.map((c) => c.chip));
  for (const chip of fresh.per_chip) {
    if (!savedNames.has(chip.chip)) differences.push(`chip ${chip.chip}: not in the saved result`);
  }
  return { sameAnswer: differences.length === 0, differences };
}
