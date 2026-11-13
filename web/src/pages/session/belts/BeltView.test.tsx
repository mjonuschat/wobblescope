import { QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { createQueryClient } from "@/lib/api/queries";
import type { BeltSide } from "@/lib/generated/BeltSide";
import type { BeltsAnalysis } from "@/lib/generated/BeltsAnalysis";
import { beltSpectrum, beltsComparison, beltsResult, twoAxisMeta } from "@/test/fixtures";

import type { Header } from "../SessionResults";
import { BeltResults } from "./BeltResults";

const fake = vi.hoisted(() => ({ analyze: vi.fn(), spectrogram: vi.fn(), belts: vi.fn(), forget: vi.fn() }));

vi.mock("@/analysis/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/analysis/client")>()),
  getAnalysisClient: () => fake,
}));
vi.mock("@/components/charts/EChart", () => ({
  EChart: ({ label, option }: { label: string; option: { yAxis?: { max?: number } } }) => (
    <div role="img" aria-label={label} data-ymax={option.yAxis?.max} />
  ),
}));

const side = (over: Partial<BeltSide> = {}): BeltSide => ({
  present: true,
  decision: null,
  failure: null,
  spectrum: beltSpectrum(),
  ...over,
});
const absent = (): BeltSide => side({ present: false, spectrum: null });

function oneBelt(present: "a" | "b", chip = "adxl345"): BeltsAnalysis {
  return {
    chip,
    point: 0,
    belt_a: present === "a" ? side() : absent(),
    belt_b: present === "b" ? side() : absent(),
    result: null,
    core_version: "0.0.0",
  };
}

const header: Header = (_save, summary) => (
  <section aria-label="Test result">
    <h2>Result</h2>
    {summary}
  </section>
);

function renderView() {
  const client = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return render(<BeltResults id="s1" meta={twoAxisMeta()} header={header} />, { wrapper });
}

beforeEach(() => {
  fake.belts.mockReset();
});

describe("the belt view", () => {
  it("it_shows_overlay_scatter_table_similarity_and_health", async () => {
    fake.belts.mockResolvedValue(beltsComparison());
    renderView();
    expect(await screen.findByRole("img", { name: "Belt overlay of adxl345" })).toBeTruthy();
    expect(screen.getByRole("img", { name: "Cross-belt scatter of adxl345" })).toBeTruthy();
    expect(screen.getByText("Estimated similarity").nextElementSibling?.textContent).toBe("87.7%");
    expect(screen.getByText("Mechanical health").nextElementSibling?.textContent).toBe("Good (experimental)");
    const rows = screen.getAllByRole("row");
    expect(
      within(rows[0])
        .getAllByRole("columnheader")
        .map((c) => c.textContent),
    ).toEqual(["Peaks", "Frequency delta", "Amplitude delta"]);
    expect(
      within(rows[1])
        .getAllByRole("cell")
        .map((c) => c.textContent),
    ).toEqual(["α", "0.0 Hz", "0.0 %"]);
    expect(
      within(rows[2])
        .getAllByRole("cell")
        .map((c) => c.textContent),
    ).toEqual(["β", "50.0 Hz", "12.3 %"]);
  });

  it("a_nan_similarity_shows_a_dash", async () => {
    fake.belts.mockResolvedValue(
      beltsComparison("adxl345", beltsResult({ similarity: Number.NaN, health: "Unknown mechanical health" })),
    );
    renderView();
    expect((await screen.findByText("Estimated similarity")).nextElementSibling?.textContent).toBe("\u2013");
    expect(screen.getByText("Mechanical health").nextElementSibling?.textContent).toBe("Unknown (experimental)");
    expect(screen.queryByText(/NaN/)).toBeNull();
  });

  it("the_similarity_is_in_the_top_card_and_not_in_the_belt_frequency_profile_card", async () => {
    fake.belts.mockResolvedValue(beltsComparison());
    renderView();
    await screen.findByRole("img", { name: "Belt overlay of adxl345" });
    const top = screen.getByRole("region", { name: "Test result" });
    expect(within(top).getByText("Estimated similarity")).toBeTruthy();
    expect(within(top).getByText("Mechanical health")).toBeTruthy();
    const comparison = screen.getByRole("region", { name: "Belt frequency profile of ADXL345" });
    expect(within(comparison).queryByText("Estimated similarity")).toBeNull();
    expect(within(comparison).queryByText("Mechanical health")).toBeNull();
  });

  it("a_comparison_shows_the_frequency_profile_card_and_the_cross_belt_card_in_order", async () => {
    fake.belts.mockResolvedValue(beltsComparison());
    renderView();
    const profile = await screen.findByRole("region", { name: "Belt frequency profile of ADXL345" });
    const cross = await screen.findByRole("region", { name: "Cross-belt comparison of ADXL345" });
    expect(within(profile).getByText("Belt Frequency Profile")).toBeTruthy();
    expect(within(profile).queryByRole("tab")).toBeNull();
    expect(within(profile).getByRole("img", { name: "Belt overlay of adxl345" })).toBeTruthy();
    expect(within(profile).queryByRole("img", { name: /scatter/ })).toBeNull();
    expect(within(cross).getByText("Cross-belt Comparison")).toBeTruthy();
    expect(within(cross).getByRole("img", { name: "Cross-belt scatter of adxl345" })).toBeTruthy();
    const top = screen.getByRole("region", { name: "Test result" });
    expect(within(top).getByRole("tab", { name: "ADXL345" })).toBeTruthy();
    expect(top.compareDocumentPosition(profile) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(profile.compareDocumentPosition(cross) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("the_unpaired_message_is_in_the_cross_belt_card_above_the_scatter", async () => {
    fake.belts.mockResolvedValue(beltsComparison("adxl345", beltsResult({ offsets: [] })));
    renderView();
    const cross = await screen.findByRole("region", { name: "Cross-belt comparison of ADXL345" });
    const message = within(cross).getByText("No peaks were paired across the belts.");
    const scatter = within(cross).getByRole("img", { name: /scatter/ });
    expect(message.compareDocumentPosition(scatter) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const profile = screen.getByRole("region", { name: "Belt frequency profile of ADXL345" });
    expect(within(profile).queryByText("No peaks were paired across the belts.")).toBeNull();
  });

  it("a_one_belt_session_renders_only_the_frequency_profile_card", async () => {
    fake.belts.mockResolvedValue(oneBelt("a"));
    renderView();
    await screen.findByText("Belt B was not recorded.");
    expect(screen.getByRole("region", { name: "Belt frequency profile of ADXL345" })).toBeTruthy();
    expect(screen.queryByText("Cross-belt Comparison")).toBeNull();
    expect(screen.queryByRole("region", { name: /Cross-belt/ })).toBeNull();
  });

  it("switching_chips_updates_both_cards", async () => {
    fake.belts.mockImplementation(async ({ chip }: { chip: string }) => beltsComparison(chip));
    renderView();
    await screen.findByRole("region", { name: "Cross-belt comparison of ADXL345" });
    fireEvent.click(screen.getByRole("tab", { name: "LIS2DW EBB36" }));
    const profile = await screen.findByRole("region", { name: "Belt frequency profile of LIS2DW EBB36" });
    const cross = await screen.findByRole("region", { name: "Cross-belt comparison of LIS2DW EBB36" });
    expect(within(profile).getByRole("img", { name: "Belt overlay of lis2dw ebb36" })).toBeTruthy();
    expect(within(cross).getByRole("img", { name: "Cross-belt scatter of lis2dw ebb36" })).toBeTruthy();
  });

  it("a_belt_a_only_session_says_belt_b_was_not_recorded_and_shows_belt_a", async () => {
    fake.belts.mockResolvedValue(oneBelt("a"));
    renderView();
    expect((await screen.findByText("Belt B was not recorded.")).getAttribute("role")).toBe("status");
    expect(screen.getByRole("img", { name: "Belt A (axis 1,-1) of adxl345" })).toBeTruthy();
    expect(screen.queryByRole("img", { name: /overlay|scatter/ })).toBeNull();
    expect(screen.queryByText("Belt A was not recorded.")).toBeNull();
    expect(screen.queryByText("Estimated similarity")).toBeNull();
    expect(screen.queryByText("Mechanical health")).toBeNull();
  });

  it("a_belt_b_only_session_says_belt_a_was_not_recorded_and_shows_belt_b", async () => {
    fake.belts.mockResolvedValue(oneBelt("b"));
    renderView();
    expect(await screen.findByText("Belt A was not recorded.")).toBeTruthy();
    expect(screen.getByRole("img", { name: "Belt B (axis 1, 1) of adxl345" })).toBeTruthy();
    expect(screen.queryByText("Belt B was not recorded.")).toBeNull();
  });

  it("a_chip_with_one_belt_shows_the_missing_side", async () => {
    fake.belts.mockImplementation(async ({ chip }: { chip: string }) =>
      chip === "adxl345" ? beltsComparison(chip) : oneBelt("a", chip),
    );
    renderView();
    await screen.findByRole("img", { name: "Belt overlay of adxl345" });
    fireEvent.click(screen.getByRole("tab", { name: "LIS2DW EBB36" }));
    expect(await screen.findByText("Belt B was not recorded.")).toBeTruthy();
    expect(screen.getByRole("img", { name: "Belt A (axis 1,-1) of lis2dw ebb36" })).toBeTruthy();
  });

  it("an_excluded_chip_shows_its_reason_and_can_be_included", async () => {
    const excluded = side({
      spectrum: null,
      decision: {
        chip: "adxl345",
        point: 0,
        decision: "Excluded",
        reasons: ["the recording is clipped"],
        dropped: null,
      },
    });
    fake.belts.mockImplementation(async (job: { includeExcluded: boolean }) =>
      job.includeExcluded ? beltsComparison() : { ...oneBelt("b"), belt_a: excluded },
    );
    renderView();
    expect(await screen.findByText(/Belt A could not be used: the recording is clipped/)).toBeTruthy();
    expect(screen.queryByText("Estimated similarity")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Include belt A anyway" }));
    expect(await screen.findByRole("img", { name: "Belt overlay of adxl345" })).toBeTruthy();
    expect(fake.belts.mock.calls.at(-1)?.[0]).toMatchObject({ includeExcluded: true });
  });

  it("including_an_excluded_recording_applies_to_that_chip_only", async () => {
    const excluded = side({
      spectrum: null,
      decision: { chip: "adxl345", point: 0, decision: "Excluded", reasons: ["clipped"], dropped: null },
    });
    fake.belts.mockImplementation(async ({ chip, includeExcluded }: { chip: string; includeExcluded: boolean }) =>
      includeExcluded ? beltsComparison(chip) : { ...oneBelt("b", chip), belt_a: excluded },
    );
    renderView();
    fireEvent.click(await screen.findByRole("button", { name: "Include belt A anyway" }));
    expect(await screen.findByRole("img", { name: "Belt overlay of adxl345" })).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "LIS2DW EBB36" }));
    await waitFor(() => expect(fake.belts.mock.calls.at(-1)?.[0]).toMatchObject({ chip: "lis2dw ebb36" }));
    expect(fake.belts.mock.calls.at(-1)?.[0]).toMatchObject({ includeExcluded: false });
  });

  it("the_chip_selector_changes_the_result", async () => {
    fake.belts.mockImplementation(async ({ chip }: { chip: string }) =>
      beltsComparison(chip, beltsResult({ health: `health of ${chip}` })),
    );
    renderView();
    expect(await screen.findByText(/health of adxl345/)).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "LIS2DW EBB36" }));
    expect(await screen.findByText(/health of lis2dw ebb36/)).toBeTruthy();
    const top = screen.getByRole("region", { name: "Test result" });
    expect(within(top).getByText(/health of lis2dw ebb36/)).toBeTruthy();
    expect(within(top).queryByText(/health of adxl345/)).toBeNull();
    expect(fake.belts.mock.calls.at(-1)?.[0]).toMatchObject({ chip: "lis2dw ebb36" });
  });

  it("asks_for_the_analysis_with_a_maximum_of_200_hz_and_has_no_analysis_maximum_field", async () => {
    fake.belts.mockResolvedValue(beltsComparison());
    renderView();
    expect(await screen.findByRole("img", { name: "Belt overlay of adxl345" })).toBeTruthy();
    expect(fake.belts.mock.calls.at(-1)?.[0]).toMatchObject({ maxFreq: 200 });
    expect(screen.queryByLabelText(/Analysis maximum/)).toBeNull();
  });

  it("has_no_chart_scale_dialog", async () => {
    fake.belts.mockResolvedValue(beltsComparison());
    renderView();
    const overlay = await screen.findByRole("img", { name: "Belt overlay of adxl345" });
    expect(Number(overlay.getAttribute("data-ymax"))).toBeCloseTo(6.6);
    expect(screen.queryByRole("button", { name: "Set the chart scale" })).toBeNull();
  });

  it("a_one_belt_session_draws_that_belt_alone_from_its_spectrum", async () => {
    for (const [belt, label] of [
      ["a", "Belt A (axis 1,-1) of adxl345"],
      ["b", "Belt B (axis 1, 1) of adxl345"],
    ] as const) {
      fake.belts.mockResolvedValue(oneBelt(belt));
      const view = renderView();
      expect(await screen.findByRole("img", { name: label })).toBeTruthy();
      expect(screen.getAllByRole("img")).toHaveLength(1);
      view.unmount();
    }
  });
});
