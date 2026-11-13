import { QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { createQueryClient } from "@/lib/api/queries";
import type { BeltSide } from "@/lib/generated/BeltSide";
import type { BeltsAnalysis } from "@/lib/generated/BeltsAnalysis";
import { beltSpectrum, beltsComparison, beltsResult, twoAxisMeta } from "@/test/fixtures";

import { BeltView } from "./BeltView";

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

function renderView() {
  const client = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return render(<BeltView id="s1" meta={twoAxisMeta()} />, { wrapper });
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
    expect(screen.getByText(/Estimated similarity: 87\.7%/)).toBeTruthy();
    expect(screen.getByText(/Mechanical health: good \(experimental\)/)).toBeTruthy();
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
    expect(
      await screen.findByText(/Estimated similarity: \u2013 \| Unknown mechanical health \(experimental\)/),
    ).toBeTruthy();
    expect(screen.queryByText(/NaN/)).toBeNull();
  });

  it("a_belt_a_only_session_says_belt_b_was_not_recorded_and_shows_belt_a", async () => {
    fake.belts.mockResolvedValue(oneBelt("a"));
    renderView();
    expect((await screen.findByText("Belt B was not recorded.")).getAttribute("role")).toBe("status");
    expect(screen.getByRole("img", { name: "Belt A (axis 1,-1) of adxl345" })).toBeTruthy();
    expect(screen.queryByRole("img", { name: /overlay|scatter/ })).toBeNull();
    expect(screen.queryByText("Belt A was not recorded.")).toBeNull();
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
    expect(fake.belts.mock.calls.at(-1)?.[0]).toMatchObject({ chip: "lis2dw ebb36" });
  });

  it("changing_the_analysis_maximum_asks_for_a_new_analysis_and_redraws", async () => {
    fake.belts.mockImplementation(async ({ maxFreq }: { maxFreq: number }) =>
      beltsComparison("adxl345", beltsResult({ health: `kept to ${maxFreq}` })),
    );
    renderView();
    const field = (await screen.findByLabelText("Analysis maximum (Hz)")) as HTMLInputElement;
    expect(field.value).toBe("200");
    expect(await screen.findByText(/kept to 200/)).toBeTruthy();

    fireEvent.change(field, { target: { value: "5" } });
    expect(screen.getByRole("alert").textContent).toContain("at least 10");
    fireEvent.change(field, { target: { value: "" } });
    expect(screen.getByRole("alert")).toBeTruthy();
    expect(fake.belts).toHaveBeenCalledTimes(1);

    fireEvent.change(field, { target: { value: "120" } });
    expect(screen.queryByRole("alert")).toBeNull();
    expect(await screen.findByText(/kept to 120/)).toBeTruthy();
    expect(fake.belts.mock.calls.at(-1)?.[0]).toMatchObject({ maxFreq: 120 });
  });

  it("an_invalid_analysis_maximum_hides_the_previous_result_until_it_is_valid_again", async () => {
    fake.belts.mockResolvedValue(beltsComparison());
    renderView();
    const field = (await screen.findByLabelText("Analysis maximum (Hz)")) as HTMLInputElement;
    expect(await screen.findByRole("img", { name: "Belt overlay of adxl345" })).toBeTruthy();

    fireEvent.change(field, { target: { value: "5" } });
    expect(screen.getByRole("alert").textContent).toContain("at least 10");
    expect(screen.getByLabelText("Analysis maximum (Hz)")).toBeTruthy();
    expect(screen.queryByRole("img", { name: "Belt overlay of adxl345" })).toBeNull();
    expect(screen.queryByRole("img", { name: "Cross-belt scatter of adxl345" })).toBeNull();
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.queryByText(/Estimated similarity/)).toBeNull();

    fireEvent.change(field, { target: { value: "200" } });
    expect(await screen.findByRole("img", { name: "Belt overlay of adxl345" })).toBeTruthy();
    expect(screen.getByText(/Estimated similarity/)).toBeTruthy();
  });

  it("has_no_chart_scale_dialog_and_the_analysis_maximum_says_it_decides_the_peaks", async () => {
    fake.belts.mockResolvedValue(beltsComparison());
    renderView();
    const overlay = await screen.findByRole("img", { name: "Belt overlay of adxl345" });
    expect(Number(overlay.getAttribute("data-ymax"))).toBeCloseTo(6.6);
    expect(screen.queryByRole("button", { name: "Set the chart scale" })).toBeNull();
    expect(screen.getByLabelText("Analysis maximum (Hz)")).toBeTruthy();
    fireEvent.focus(screen.getByRole("button", { name: "About analysis maximum (hz)" }));
    expect(
      (await screen.findAllByText(/The comparison covers 0 to this frequency. It decides which peaks are found./))
        .length,
    ).toBeGreaterThan(0);
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
