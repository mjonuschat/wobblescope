import { describe, expect, it } from "vitest";

import { analysis, candidate, recommendation } from "@/test/fixtures";

import { compareAnalyses } from "./compare";

describe("compareAnalyses", () => {
  it("reports the same answer for identical analyses", () => {
    expect(compareAnalyses(analysis(), analysis())).toEqual({ sameAnswer: true, differences: [] });
  });

  it("names a changed shaper frequency", () => {
    const fresh = analysis();
    fresh.worst_case = recommendation(candidate("mzv", 50));
    const { sameAnswer, differences } = compareAnalyses(analysis(), fresh);
    expect(sameAnswer).toBe(false);
    expect(differences).toContain("worst case: freq 48.2 -> 50");
  });

  it("names a changed max_accel when name and frequency are the same", () => {
    const fresh = analysis();
    fresh.per_chip[0].recommendation = recommendation(candidate("mzv", 48.2, { max_accel: 4200 }));
    const { differences } = compareAnalyses(analysis(), fresh);
    expect(differences).toEqual(["chip a: max_accel 4500 -> 4200"]);
  });

  it("ignores a difference below the tolerance", () => {
    const fresh = analysis();
    fresh.worst_case = recommendation(candidate("mzv", 48.2 * (1 + 1e-8), { dataset_vibrs: [0.032 * (1 + 1e-8)] }));
    expect(compareAnalyses(analysis(), fresh)).toEqual({ sameAnswer: true, differences: [] });
  });

  it("names a changed dataset_vibrs entry", () => {
    const fresh = analysis();
    fresh.worst_case = recommendation(candidate("mzv", 48.2, { dataset_vibrs: [0.5] }));
    expect(compareAnalyses(analysis(), fresh).differences).toEqual(["worst case: dataset_vibrs [0.032] -> [0.5]"]);
  });

  it("names a chip that gained or lost its recommendation", () => {
    const fresh = analysis();
    fresh.per_chip[0].recommendation = null;
    fresh.per_chip[1].recommendation = recommendation();
    expect(compareAnalyses(analysis(), fresh).differences).toEqual([
      "chip a: recommendation lost",
      "chip b: recommendation gained",
    ]);
  });

  it("names a chip present on one side only and a lost worst case", () => {
    const fresh = analysis();
    fresh.per_chip.pop();
    fresh.worst_case = null;
    expect(compareAnalyses(analysis(), fresh).differences).toEqual([
      "worst case: recommendation lost",
      "chip b: missing from the re-analysis",
    ]);
  });
});
