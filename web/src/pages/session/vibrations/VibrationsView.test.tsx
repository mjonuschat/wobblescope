import { QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { createQueryClient } from "@/lib/api/queries";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import type { VibrationsAnalysis } from "@/lib/generated/VibrationsAnalysis";
import { twoAxisMeta } from "@/test/fixtures";
import { motorResult, vibrationsResult } from "@/test/vibrations";

import type { Header } from "../SessionResults";
import { VibrationsResults } from "./VibrationsResults";

const fake = vi.hoisted(() => ({
  analyze: vi.fn(),
  spectrogram: vi.fn(),
  belts: vi.fn(),
  vibrations: vi.fn(),
  forget: vi.fn(),
}));

vi.mock("@/analysis/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/analysis/client")>()),
  getAnalysisClient: () => fake,
}));
vi.mock("@/components/charts/EChart", () => ({
  EChart: ({ label }: { label: string }) => <div role="img" aria-label={label} />,
}));

function vibrationsMeta(): SessionMeta {
  const meta = twoAxisMeta();
  meta.vibrations_run = {
    size: 100,
    z_height: 20,
    max_speed: 100,
    speed_increment: 20,
    accel: 3000,
    travel_speed: 120,
    min_speed: 20,
    kinematics: "corexy",
  };
  return meta;
}

const analysis = (over: Partial<VibrationsAnalysis> = {}): VibrationsAnalysis => ({
  per_chip: [
    { chip: "adxl345", decisions: [], excluded: null, result: vibrationsResult(), failure: null },
    { chip: "lis2dw ebb36", decisions: [], excluded: null, result: vibrationsResult(), failure: null },
  ],
  worst_case: vibrationsResult({ motor: null }),
  core_version: "0.0.0",
  ...over,
});

const header: Header = (_save, summary) => (
  <section aria-label="Test result">
    <h2>Result</h2>
    {summary}
  </section>
);

function renderView(meta = vibrationsMeta()) {
  const client = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return render(<VibrationsResults id="s1" meta={meta} header={header} />, { wrapper });
}

const panel = (name: string) => screen.queryByRole("region", { name });

beforeEach(() => {
  fake.vibrations.mockReset();
});

describe("the vibrations view", () => {
  it("has_a_tab_per_chip_and_all_chips_selected", async () => {
    fake.vibrations.mockResolvedValue(analysis());
    renderView();
    expect(await screen.findByRole("tab", { name: "All chips" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "All chips" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tab", { name: "ADXL345" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "LIS2DW EBB36" })).toBeTruthy();
    expect(await screen.findByRole("region", { name: "Speed energy profile" })).toBeTruthy();
    expect(panel("Motor resonance profile")).toBeNull();
    expect(panel("Summary")).toBeNull();
    expect(
      within(screen.getByRole("region", { name: "Test result" })).getByText("Recommended speed ranges"),
    ).toBeTruthy();
    expect(fake.vibrations).toHaveBeenCalledTimes(1);
  });

  it("offers_the_angle_presets_above_the_angular_speed_profiles_with_motor_directions_selected", async () => {
    fake.vibrations.mockResolvedValue(analysis());
    renderView();
    await screen.findByRole("region", { name: "Angular speed profiles" });
    const group = within(screen.getByRole("region", { name: "Angular speed profiles" })).getByRole("group", {
      name: "Angles",
    });
    const motors = within(group).getByRole("button", { name: "Motor directions" });
    const steps = within(group).getByRole("button", { name: "Every 22.5 degrees" });
    expect(within(group).getByRole("button", { name: "Every 15 degrees" })).toBeTruthy();
    expect(motors.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(steps);
    expect(steps.getAttribute("aria-pressed")).toBe("true");
    expect(motors.getAttribute("aria-pressed")).toBe("false");
  });

  it("shows_the_motor_panel_on_a_chip_tab", async () => {
    fake.vibrations.mockResolvedValue(analysis());
    renderView();
    fireEvent.click(await screen.findByRole("tab", { name: "ADXL345" }));
    expect(await screen.findByRole("region", { name: "Motor resonance profile" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Speed energy profile" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Energy heatmap" })).toBeTruthy();
    expect(fake.vibrations).toHaveBeenCalledTimes(1);
  });

  it("shows_the_recommended_speed_ranges_in_the_result_card_and_the_angle_ranges_beside_their_plot", async () => {
    fake.vibrations.mockResolvedValue(analysis());
    renderView();
    await screen.findByRole("region", { name: "Speed energy profile" });
    const result = screen.getByRole("region", { name: "Test result" });
    expect(within(result).getByText("Recommended speed ranges")).toBeTruthy();
    expect(within(result).queryByText("Speeds to avoid")).toBeNull();
    expect(within(result).queryByText("Good angle ranges")).toBeNull();
    expect(
      within(screen.getByRole("region", { name: "Speed energy profile" })).queryByText("Recommended speed ranges"),
    ).toBeNull();
    expect(within(screen.getByRole("region", { name: "Angular energy" })).getByText("Good angle ranges")).toBeTruthy();
  });

  it("shows_the_resonance_and_damping_in_the_motor_card_and_not_in_the_chart", async () => {
    fake.vibrations.mockResolvedValue(analysis());
    renderView();
    fireEvent.click(await screen.findByRole("tab", { name: "ADXL345" }));
    const motor = await screen.findByRole("region", { name: "Motor resonance profile" });
    expect(within(motor).getByText("Resonant frequency").parentElement?.textContent).toContain("30.0 Hz");
    expect(within(motor).getByText("Damping ratio").parentElement?.textContent).toContain("0.123");
    expect(within(motor).queryByText(/too high an acceleration/)).toBeNull();
    expect(within(screen.getByRole("region", { name: "Test result" })).queryByText("Motor resonance")).toBeNull();
  });

  it("warns_in_the_motor_card_about_low_frequency_vibrations", async () => {
    const data = analysis();
    data.per_chip[0].result = { ...data.per_chip[0].result!, motor: motorResult({ low_freq_warning: true }) };
    fake.vibrations.mockResolvedValue(data);
    renderView();
    fireEvent.click(await screen.findByRole("tab", { name: "ADXL345" }));
    const motor = await screen.findByRole("region", { name: "Motor resonance profile" });
    expect(within(motor).getByText(/too high an acceleration/)).toBeTruthy();
  });

  it("offers_the_expand_button_the_other_session_charts_have_and_no_zoom_buttons", async () => {
    fake.vibrations.mockResolvedValue(analysis());
    renderView();
    await screen.findByRole("region", { name: "Speed energy profile" });
    for (const name of [
      "Angular energy",
      "Polar energy heatmap",
      "Speed energy profile",
      "Angular speed profiles",
      "Energy heatmap",
    ]) {
      expect(
        within(screen.getByRole("region", { name })).getByRole("button", { name: /Expand the chart/ }),
      ).toBeTruthy();
    }
    expect(screen.queryByRole("button", { name: /zoom/i })).toBeNull();
  });

  it("shows_why_a_chip_is_left_out_and_still_shows_its_charts", async () => {
    const data = analysis();
    data.per_chip[1] = { ...data.per_chip[1], excluded: "degraded capture health" };
    fake.vibrations.mockResolvedValue(data);
    renderView();
    fireEvent.click(await screen.findByRole("tab", { name: "LIS2DW EBB36" }));
    expect(await screen.findByText(/The capture looks unreliable/)).toBeTruthy();
    expect(screen.getByRole("region", { name: "Speed energy profile" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Include excluded chips anyway" }));
    expect(await screen.findByRole("tab", { name: "All chips" })).toBeTruthy();
    expect(fake.vibrations).toHaveBeenLastCalledWith(
      expect.objectContaining({ includeExcluded: true }),
      expect.anything(),
    );
  });

  it("shows_a_chip_failure_and_that_it_is_not_in_all_chips", async () => {
    const data = analysis();
    data.per_chip[0] = {
      chip: "adxl345",
      decisions: [],
      excluded: "left out of All chips: no samples",
      result: null,
      failure: "no samples",
    };
    fake.vibrations.mockResolvedValue(data);
    renderView();
    fireEvent.click(await screen.findByRole("tab", { name: "ADXL345" }));
    expect(await screen.findByText(/no samples/)).toBeTruthy();
    expect(screen.getByText(/not included in All chips/)).toBeTruthy();
    expect(screen.queryByText(/left out of All chips/)).toBeNull();
    expect(screen.queryByRole("button", { name: /Include excluded chips anyway/ })).toBeNull();
    expect(panel("Speed energy profile")).toBeNull();
  });

  it("shows_an_empty_state_when_no_chip_qualifies_for_all_chips", async () => {
    fake.vibrations.mockResolvedValue(analysis({ worst_case: null }));
    renderView();
    expect(await screen.findByText("No chip qualifies for All chips.")).toBeTruthy();
    expect(panel("Speed energy profile")).toBeNull();
  });

  it("shows_progress_for_an_incomplete_session_and_does_not_analyse", () => {
    const meta = vibrationsMeta();
    meta.outcome = { Failed: { reason: "stopped by the user" } };
    meta.segments = [
      {
        ...meta.segments[0],
        steps: Array.from({ length: 5 }, (_, i) => ({ speed: 20 + 20 * i, window: { start: 0, end: 1 } })),
      },
      { ...meta.segments[1], steps: [{ speed: 20, window: { start: 0, end: 1 } }] },
    ] as SessionMeta["segments"];
    renderView(meta);
    expect(screen.getByText("Angle 2 of 2, speed 1 of 5")).toBeTruthy();
    expect(screen.getByText(/stopped by the user/)).toBeTruthy();
    expect(screen.queryAllByRole("img")).toHaveLength(0);
    expect(screen.queryByRole("tab")).toBeNull();
    expect(fake.vibrations).not.toHaveBeenCalled();
  });

  it("shows_progress_and_the_reason_of_an_incomplete_outcome_and_does_not_analyse", () => {
    const meta = vibrationsMeta();
    meta.outcome = { Incomplete: { reason: "the printer lost power" } };
    meta.segments = [
      {
        ...meta.segments[0],
        steps: Array.from({ length: 5 }, (_, i) => ({ speed: 20 + 20 * i, window: { start: 0, end: 1 } })),
      },
    ] as SessionMeta["segments"];
    renderView(meta);
    expect(screen.getByText("Angle 1 of 2 finished, no step of angle 2 finished")).toBeTruthy();
    expect(screen.getByText("Reason: the printer lost power")).toBeTruthy();
    expect(screen.queryAllByRole("img")).toHaveLength(0);
    expect(screen.queryByRole("tab")).toBeNull();
    expect(fake.vibrations).not.toHaveBeenCalled();
  });
});
