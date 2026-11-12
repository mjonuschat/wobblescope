import { describe, expect, it } from "vitest";

import type { DatasetDecision } from "@/lib/generated/DatasetDecision";

import { explainReason, verdict } from "./EligibilityPanel";

describe("explainReason", () => {
  it("puts the analysis's health reasons in plain words", () => {
    expect(explainReason("degraded capture health")).toBe(
      "The capture looks unreliable: dropped samples, timing gaps or suspect values. The session's health notes say which.",
    );
  });

  it("explains an incomplete capture with what is missing", () => {
    expect(explainReason("incomplete coverage: no samples after 41 s")).toBe(
      "The capture is incomplete: no samples after 41 s",
    );
  });

  it("leaves a reason it does not know as it was written", () => {
    const rate = "samples arrive at 395 Hz, under half the 1600 Hz Klipper configures";
    expect(explainReason(rate)).toBe(rate);
  });
});

const decision = (over: Partial<DatasetDecision>): DatasetDecision => ({
  chip: "a",
  point: 0,
  decision: "Included",
  reasons: [],
  dropped: null,
  ...over,
});

describe("verdict", () => {
  it("calls an included recording with nothing to say about it good", () => {
    expect(verdict(decision({}))).toEqual({ tone: "good", headline: "Good data", detail: "" });
  });

  it("says why a recording was left out, and when it was included by the user", () => {
    expect(verdict(decision({ decision: "Excluded", reasons: ["degraded capture health"] }))).toEqual({
      tone: "bad",
      headline: "Left out",
      detail:
        "The capture looks unreliable: dropped samples, timing gaps or suspect values. The session's health notes say which.",
    });
    expect(verdict(decision({ decision: "Overridden", reasons: ["degraded capture health"] })).headline).toBe(
      "Included at your request",
    );
  });
});
