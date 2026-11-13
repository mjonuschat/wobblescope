import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { createQueryClient } from "@/lib/api/queries";
import type { SessionKind } from "@/lib/generated/SessionKind";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import { analysis, beltsComparison, decision, twoAxisMeta } from "@/test/fixtures";
import { stubApiWithStatus as stubApi } from "@/test/status";
import { summary } from "@/test/summaries";

import { SessionResults } from "./SessionResults";

const fake = vi.hoisted(() => ({ analyze: vi.fn(), spectrogram: vi.fn(), belts: vi.fn(), forget: vi.fn() }));

vi.mock("@/analysis/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/analysis/client")>()),
  getAnalysisClient: () => fake,
}));
vi.mock("@/components/charts/EChart", () => ({
  EChart: ({ label }: { label: string }) => <div role="img" aria-label={label} />,
}));

function vectorMeta(): SessionMeta {
  const meta = twoAxisMeta();
  meta.probe_points = [{ position: [10, 20, 30] }, { position: null }];
  meta.segments = [
    { ...meta.segments[0], axis: { Vector: [1, -1, 0] }, point: 0 },
    { ...meta.segments[0], axis: { Vector: [1, 1, 0] }, point: 1 },
    { ...meta.segments[1], axis: "X", point: 0 },
  ];
  return meta;
}

function serve(kind: SessionKind, meta: SessionMeta) {
  stubApi(({ url }) =>
    url.pathname.endsWith("/sessions/s1")
      ? { body: { summary: summary("s1", { kind }), meta } }
      : { status: 404, body: { error: "not_found", message: "no such session" } },
  );
}

function renderResults() {
  render(
    <QueryClientProvider client={createQueryClient()}>
      <SessionResults id="s1" />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  fake.analyze.mockReset();
  fake.belts.mockReset();
});

describe("the session results", () => {
  it("axes_renders_the_shaper_view", async () => {
    const a = analysis();
    a.per_chip = [{ ...a.per_chip[0], chip: "adxl345" }];
    a.eligibility.decisions = [decision("adxl345", "Included", [])];
    fake.analyze.mockResolvedValue({
      analysis: a,
      snippets: { worstCase: null, worstCasePerformance: null, perChip: [null] },
    });
    serve("axes", twoAxisMeta());
    renderResults();
    expect(await screen.findByRole("tab", { name: "Axis X" })).toBeTruthy();
    expect(fake.belts).not.toHaveBeenCalled();
  });

  it("belts_renders_the_belt_view", async () => {
    fake.belts.mockResolvedValue(beltsComparison());
    serve("belts", vectorMeta());
    renderResults();
    expect(await screen.findByRole("img", { name: "Belt overlay of adxl345" })).toBeTruthy();
    expect(screen.queryByRole("tab", { name: "Axis X" })).toBeNull();
    expect(fake.analyze).not.toHaveBeenCalled();
  });

  it("belts_history_page_shows_the_similarity_in_a_result_card_at_the_top", async () => {
    fake.belts.mockResolvedValue(beltsComparison());
    serve("belts", vectorMeta());
    renderResults();
    const top = await screen.findByRole("region", { name: "Result" });
    expect((await within(top).findByText("Estimated similarity")).nextElementSibling?.textContent).toBe("87.7%");
    expect(within(top).getByRole("tab", { name: "ADXL345" })).toBeTruthy();
    expect(within(top).getByText("Mechanical health")).toBeTruthy();
  });

  it("belts_history_page_shows_the_chip_tabs_but_no_similarity_without_a_comparison", async () => {
    fake.belts.mockResolvedValue({ ...beltsComparison(), result: null });
    serve("belts", vectorMeta());
    renderResults();
    await screen.findByText("There is no comparison without both belts.");
    const top = screen.getByRole("region", { name: "Result" });
    expect(within(top).getByRole("tab", { name: "ADXL345" })).toBeTruthy();
    expect(within(top).queryByText("Estimated similarity")).toBeNull();
  });

  it("other_renders_the_no_analysis_message_and_lists_the_segments", async () => {
    serve("other", vectorMeta());
    renderResults();
    expect(await screen.findByText("This version has no analysis for these directions.")).toBeTruthy();
    const items = within(screen.getByRole("list", { name: "Segments" })).getAllByRole("listitem");
    expect(items.map((i) => i.textContent)).toEqual([
      "Direction 1,-1 at (10, 20, 30)",
      "Direction 1,1 at point 1",
      "Direction X at (10, 20, 30)",
    ]);
    expect(fake.analyze).not.toHaveBeenCalled();
    expect(fake.belts).not.toHaveBeenCalled();
  });
});
