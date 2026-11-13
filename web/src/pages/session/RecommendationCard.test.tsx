import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { FitSettings } from "@/lib/generated/FitSettings";
import { candidate, recommendation, settings } from "@/test/fixtures";

import { RecommendationCard } from "./RecommendationCard";

describe("RecommendationCard", () => {
  it("shows the max acceleration to the nearest 100 like Klipper", () => {
    const rec = recommendation(candidate("mzv", 48.2, { max_accel: 4537 }));
    render(<RecommendationCard title="t" recommendation={rec} snippet={null} />);
    expect(screen.getByText("4500 mm/s²")).toBeTruthy();
  });

  it("lists each chip behind a worst case and marks the one that sets it", () => {
    render(
      <RecommendationCard
        title="t"
        recommendation={recommendation()}
        snippet={null}
        datasetVibrations={[
          { chip: "a", point: 0, vibrs: 0.01 },
          { chip: "b", point: 0, vibrs: 0.032 },
        ]}
      />,
    );
    const items = screen.getAllByRole("listitem");
    expect(items.map((i) => i.textContent)).toEqual(["a: 1.0%", "b: 3.2%"]);
    expect(items.map((i) => i.getAttribute("data-limiting"))).toEqual(["false", "true"]);
  });

  it("shows no per-chip list for a single dataset", () => {
    render(
      <RecommendationCard
        title="t"
        recommendation={recommendation()}
        snippet={null}
        datasetVibrations={[{ chip: "a", point: 0, vibrs: 0.032 }]}
      />,
    );
    expect(screen.queryByRole("listitem")).toBeNull();
  });

  it("explains the max acceleration in a tooltip on its label", async () => {
    render(<RecommendationCard title="t" recommendation={recommendation()} snippet={null} />);
    expect(screen.queryByText(/not a limit of your printer/)).toBeNull();
    fireEvent.focus(screen.getByRole("button", { name: "About max acceleration" }));
    expect(await screen.findByText(/not a limit of your printer/)).toBeTruthy();
  });

  const withPerformance = () => ({
    ...recommendation(),
    for_performance: candidate("zv", 55, { max_accel: 6000, vibrs: 0.04 }),
  });

  it("puts the damping ratio in the row with the vibrations, smoothing and acceleration", () => {
    render(<RecommendationCard title="t" recommendation={recommendation()} snippet={null} />);
    const row = screen.getByText("Remaining vibrations").closest("dl");
    expect(row).toBe(screen.getByText("Damping ratio").closest("dl"));
    expect(row).toBe(screen.getByText("Max acceleration").closest("dl"));
  });

  it("shows one block when there is no performance recommendation", () => {
    render(<RecommendationCard title="t" recommendation={recommendation()} snippet="LOW" />);
    expect(screen.queryByText("For low vibrations")).toBeNull();
    expect(screen.queryByText("For performance")).toBeNull();
  });

  it("shows both blocks with their own snippets when there is one", () => {
    render(
      <RecommendationCard
        title="t"
        recommendation={withPerformance()}
        snippet="LOW SNIPPET"
        performanceSnippet="PERF SNIPPET"
      />,
    );
    expect(screen.getByText("For low vibrations")).toBeTruthy();
    expect(screen.getByText("For performance")).toBeTruthy();
    expect(screen.getByText("mzv at 48.2 Hz")).toBeTruthy();
    expect(screen.getByText("zv at 55.0 Hz")).toBeTruthy();
    expect(screen.getByText("LOW SNIPPET")).toBeTruthy();
    expect(screen.getByText("PERF SNIPPET")).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Copy" })).toHaveLength(2);
  });

  const withDamping = (over: Partial<FitSettings>) => ({
    ...recommendation(),
    settings: { ...settings, ...over },
  });

  it("says whether the damping ratio was estimated, fell back or was set", () => {
    const shown = (over: Partial<FitSettings>) => {
      const { unmount } = render(<RecommendationCard title="t" recommendation={withDamping(over)} snippet={null} />);
      const text = screen.getByText(/^0\.\d{3} \(/).textContent;
      unmount();
      return text;
    };
    expect(shown({ damping_ratio: 0.045, damping_source: "Estimated", resonance_freq: 48.4 })).toBe(
      "0.045 (Estimated)",
    );
    expect(shown({ damping_ratio: 0.1, damping_source: "Fallback", resonance_freq: 0 })).toBe(
      "0.100 (Fell back to 0.1)",
    );
    expect(shown({ damping_ratio: 0.08, damping_source: "Fixed", resonance_freq: null })).toBe("0.080 (Set)");
  });

  it("shows the resonance frequency whenever one is known, estimated or not", () => {
    const { rerender } = render(
      <RecommendationCard
        title="t"
        recommendation={withDamping({ damping_source: "Estimated", resonance_freq: 48.4 })}
        snippet={null}
      />,
    );
    expect(screen.getByText("Resonance")).toBeTruthy();
    expect(screen.getByText("48.4 Hz")).toBeTruthy();
    rerender(
      <RecommendationCard
        title="t"
        recommendation={withDamping({ damping_source: "Fallback", resonance_freq: 48.4 })}
        snippet={null}
      />,
    );
    expect(screen.getByText("Resonance")).toBeTruthy();
    expect(screen.getByText("48.4 Hz")).toBeTruthy();
    rerender(
      <RecommendationCard
        title="t"
        recommendation={withDamping({ damping_source: "Fixed", resonance_freq: null })}
        snippet={null}
      />,
    );
    expect(screen.queryByText("Resonance")).toBeNull();
  });

  it("shows no score, which says nothing to a person", () => {
    render(<RecommendationCard title="t" recommendation={recommendation()} snippet={null} />);
    expect(screen.queryByText("Score")).toBeNull();
  });
});
