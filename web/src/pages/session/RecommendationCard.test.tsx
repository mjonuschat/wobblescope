import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { candidate, recommendation } from "@/test/fixtures";

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

  it("shows no score, which says nothing to a person", () => {
    render(<RecommendationCard title="t" recommendation={recommendation()} snippet={null} />);
    expect(screen.queryByText("Score")).toBeNull();
  });
});
