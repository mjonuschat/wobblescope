import { describe, expect, it } from "vitest";

import { explainReason } from "./EligibilityPanel";

describe("explainReason", () => {
  it("puts the analysis's health reasons in plain words", () => {
    expect(explainReason("degraded capture health")).toBe("The chip dropped or garbled samples during the capture.");
    expect(explainReason("unknown capture health")).toContain("no record of whether samples were dropped");
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
