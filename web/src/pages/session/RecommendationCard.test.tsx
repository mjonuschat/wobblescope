import { fireEvent, render, screen, within } from "@testing-library/react";
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

  it("puts the copy button on the config block itself", () => {
    render(<RecommendationCard title="t" recommendation={recommendation()} snippet="THE CONFIG" />);
    const block = screen.getByText("THE CONFIG").parentElement as HTMLElement;
    expect(within(block).getByRole("button", { name: "Copy" })).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Copy" })).toHaveLength(1);
  });

  it("shows one block when there is no performance recommendation", () => {
    render(<RecommendationCard title="t" recommendation={recommendation()} snippet="LOW" />);
    expect(screen.queryByText("For low vibrations")).toBeNull();
    expect(screen.queryByText("For performance")).toBeNull();
  });

  it("puts the two recommendations side by side in sub-cards", () => {
    render(<RecommendationCard title="t" recommendation={withPerformance()} snippet="LOW" performanceSnippet="PERF" />);
    const low = screen.getByRole("region", { name: "For low vibrations" });
    const perf = screen.getByRole("region", { name: "For performance" });
    expect(low.parentElement).toBe(perf.parentElement);
    expect(low.parentElement?.className).toContain("lg:grid-cols-2");
    expect(within(low).getByText("LOW")).toBeTruthy();
    expect(within(perf).getByText("PERF")).toBeTruthy();
  });

  it("puts the per-chip figures under the remaining vibrations value, with no heading of their own", () => {
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
    const field = screen.getByText("Remaining vibrations").parentElement as HTMLElement;
    expect(within(field).getByRole("list")).toBeTruthy();
    expect(screen.queryByText("Remaining vibrations by chip")).toBeNull();
  });

  it("shows each chip's figure under the remaining vibrations in both sub-cards", () => {
    const rec = {
      ...recommendation(),
      for_performance: candidate("zv", 55, { vibrs: 0.041, dataset_vibrs: [0.02, 0.041] }),
    };
    render(
      <RecommendationCard
        title="t"
        recommendation={rec}
        snippet={null}
        datasetVibrations={[
          { chip: "a", point: 0, vibrs: 0.01 },
          { chip: "b", point: 0, vibrs: 0.032 },
        ]}
      />,
    );
    const items = (name: string) =>
      within(screen.getByRole("region", { name }))
        .getAllByRole("listitem")
        .map((i) => i.textContent);
    expect(items("For low vibrations")).toEqual(["a: 1.0%", "b: 3.2%"]);
    expect(items("For performance")).toEqual(["a: 2.0%", "b: 4.1%"]);
  });

  it("lists the chips behind a worst case on one line", () => {
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
    expect(screen.getByRole("list").className).toContain("flex-wrap");
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

  it("explains in a tooltip where the damping ratio came from", async () => {
    const hint = async (over: Partial<FitSettings>) => {
      const { unmount } = render(<RecommendationCard title="t" recommendation={withDamping(over)} snippet={null} />);
      fireEvent.focus(screen.getByRole("button", { name: "About damping ratio" }));
      const text = (await screen.findAllByText(/Hz|estimate/)).map((e) => e.textContent).join(" ");
      unmount();
      return text;
    };
    expect(await hint({ damping_source: "Estimated", resonance_freq: 48.4 })).toContain(
      "Estimated from the strongest resonance, at 48.4 Hz.",
    );
    expect(await hint({ damping_source: "Fallback", resonance_freq: 48.4 })).toContain("no estimate");
    render(
      <RecommendationCard
        title="t"
        recommendation={withDamping({ damping_source: "Fixed", resonance_freq: null })}
        snippet={null}
      />,
    );
    expect(screen.queryByRole("button", { name: "About damping ratio" })).toBeNull();
  });

  it("shows no separate resonance row", () => {
    render(
      <RecommendationCard
        title="t"
        recommendation={withDamping({ damping_source: "Estimated", resonance_freq: 48.4 })}
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
