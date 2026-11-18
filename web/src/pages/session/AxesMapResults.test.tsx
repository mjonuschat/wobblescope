import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createAnalysisClient, type WorkerLike } from "@/analysis/client";
import { createHandler } from "@/analysis/handler";
import type { WorkerResponse } from "@/analysis/protocol";
import { chipLabel } from "@/lib/format";
import type { AxesMapChip } from "@/lib/generated/AxesMapChip";
import type { AxesMapResult } from "@/lib/generated/AxesMapResult";
import type { AxisMatch } from "@/lib/generated/AxisMatch";
import type { ChipMeta } from "@/lib/generated/ChipMeta";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import { calibrationFile, calibrationMeta, realWasm } from "@/test/calibration";
import { renderRoute } from "@/test/render";
import { stubApiWithStatus } from "@/test/status";
import { summary } from "@/test/summaries";

const fake = vi.hoisted(() => ({ analyze: vi.fn(), analyzeAxesMap: vi.fn(), forget: vi.fn() }));

vi.mock("@/analysis/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/analysis/client")>()),
  getAnalysisClient: () => fake,
}));
vi.mock("@/components/charts/EChart", () => ({
  EChart: ({ label }: { label: string }) => <div role="img" aria-label={label} />,
}));

const NO_Z_LIMITS = "Z cannot be judged without Klipper's Z limits (max_z_velocity and max_z_accel).";

/** A worker that runs the real handler over the real wasm build, fed the calibration test files. */
function realClient() {
  const handle = createHandler(async () => realWasm());
  const worker: WorkerLike = {
    onmessage: null,
    onerror: null,
    terminate() {},
    postMessage(message) {
      void handle(message).then((response) => {
        if (response) this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>);
      });
    },
  };
  return createAnalysisClient({
    createWorker: () => worker,
    fetchSample: async (_id, chip, segment) => calibrationFile(chip, segment),
  });
}

function chipMeta(name: string, over: Partial<ChipMeta> = {}): ChipMeta {
  return {
    name,
    chip_type: "adxl345",
    nominal_rate_hz: 3200,
    observed_rate_hz: 3200,
    axes_map: null,
    mounting: "toolhead",
    health: "Ok",
    health_notes: [],
    ...over,
  };
}

function sessionMeta(chips: ChipMeta[]): SessionMeta {
  return { ...calibrationMeta("s1"), chips };
}

function axis(over: Partial<AxisMatch> = {}): AxisMatch {
  return {
    state: "measured",
    chip_axis: "X",
    negative: false,
    confidence: 0.87,
    tilt_deg: 1.2,
    peaks: [80, 2, 1],
    peak_velocity: 80,
    peak_time_s: 0.2,
    ...over,
  };
}

const notDetermined = (): AxisMatch =>
  axis({ state: "not_determined", chip_axis: null, confidence: 0, tilt_deg: 0, peaks: [0, 0, 0], peak_velocity: 0 });

const curves = (): AxesMapResult["curves"] =>
  [0, 1, 2].map(() => ({
    t: [0, 0.1],
    velocity: [
      [0, 0],
      [0, 0],
      [0, 0],
    ],
  })) as AxesMapResult["curves"];

function result(chip: string, over: Partial<AxesMapResult> = {}): AxesMapResult {
  return {
    chip,
    state: "complete",
    map: "x, -z, y",
    configured_map: null,
    axes: [
      axis({ chip_axis: "X" }),
      axis({ chip_axis: "Z", negative: true, peak_velocity: -79 }),
      axis({ chip_axis: "Y" }),
    ],
    euler_deg: [90, 0, 0],
    rotation: [
      [1, 0, 0],
      [0, 0, -1],
      [0, 1, 0],
    ],
    gravity: 9.812,
    noise: 120,
    noise_verdict: "Fine",
    quality: { status: "Ok", messages: [] },
    config_check: "does_not_match",
    curves: curves(),
    core_version: "0.1.0",
    ...over,
  };
}

function analysed(
  chip: string,
  over: Partial<AxesMapResult> = {},
  health: Extract<AxesMapChip, { state: "Analysed" }>["health"] = "Ok",
  notes: string[] = [],
): AxesMapChip {
  return { state: "Analysed", chip, health, health_notes: notes, result: result(chip, over) };
}

function noMotion(chip: string, messages = ["No motion seen by this chip.", NO_Z_LIMITS]): AxesMapChip {
  return analysed(chip, {
    state: "no_motion",
    map: null,
    axes: [notDetermined(), notDetermined(), notDetermined()],
    euler_deg: null,
    rotation: null,
    quality: { status: "Error", messages },
  });
}

function serve(meta: SessionMeta) {
  stubApiWithStatus(({ url }) =>
    url.pathname.endsWith("/sessions/s1")
      ? { body: { summary: summary("s1", { kind: "axesmap", mode: null }), meta } }
      : { status: 404, body: { error: "not_found", message: "no such session" } },
  );
}

function show(meta: SessionMeta, outcomes: AxesMapChip[]) {
  fake.analyzeAxesMap.mockResolvedValue(outcomes);
  serve(meta);
  renderRoute("/sessions/s1");
}

async function openTab(name: string) {
  fireEvent.click(await screen.findByRole("tab", { name: chipLabel(name) }));
  return screen.findByRole("region", { name: `Chip ${name}` });
}

async function summaryText(): Promise<string> {
  return (await screen.findByRole("region", { name: "Result" })).textContent ?? "";
}

/** The text of the snippet block on screen, or null when there is none. */
function snippetText(): string | null {
  return document.querySelector("pre")?.textContent ?? null;
}

function clipboard(value: unknown) {
  Object.defineProperty(navigator, "clipboard", { value, configurable: true });
}

beforeEach(() => {
  fake.analyzeAxesMap.mockReset();
});
afterEach(() => clipboard(undefined));

describe("the axes-map result page", () => {
  it("one_tab_per_chip", async () => {
    const client = realClient();
    fake.analyzeAxesMap.mockImplementation((job, signal) => client.analyzeAxesMap(job, signal));
    serve(calibrationMeta("s1"));
    renderRoute("/sessions/s1");
    expect((await screen.findAllByRole("tab")).map((t) => t.textContent)).toEqual(["A", "B"]);
  });

  it("a_complete_result_shows_a_copyable_snippet", async () => {
    const client = realClient();
    fake.analyzeAxesMap.mockImplementation((job, signal) => client.analyzeAxesMap(job, signal));
    const writeText = vi.fn().mockResolvedValue(undefined);
    clipboard({ writeText });
    serve(calibrationMeta("s1"));
    renderRoute("/sessions/s1");
    await openTab("b");
    await waitFor(() => expect(snippetText()).toBe("[b]\naxes_map: y, x, z\n"));
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("[b]\naxes_map: y, x, z\n"));
  });

  it("a_partial_result_lists_determined_axes_and_offers_no_snippet", async () => {
    show(sessionMeta([chipMeta("adxl345 bed", { mounting: "bed" })]), [
      analysed("adxl345 bed", {
        state: "partial",
        map: null,
        axes: [notDetermined(), axis({ chip_axis: "Y", negative: true }), notDetermined()],
        euler_deg: null,
        rotation: null,
        quality: {
          status: "Warning",
          messages: ["Only the Y axis was seen moving; the other axes could not be determined."],
        },
      }),
    ]);
    await openTab("adxl345 bed");
    const rows = (await screen.findAllByRole("row")).map((r) => r.textContent);
    expect(rows.find((r) => r?.startsWith("Y"))).toContain("-y");
    expect(rows.find((r) => r?.startsWith("X"))).toContain("not determined");
    expect(rows.find((r) => r?.startsWith("Z"))).toContain("not determined");
    expect(await summaryText()).toContain("Only the Y axis was seen moving");
    expect(snippetText()).toBeNull();
    expect(screen.queryByRole("button", { name: "Copy" })).toBeNull();
  });

  it("no_motion_and_ambiguous_offer_no_snippet", async () => {
    show(sessionMeta([chipMeta("still"), chipMeta("twisted")]), [
      noMotion("still"),
      analysed("twisted", {
        state: "ambiguous",
        map: null,
        euler_deg: null,
        rotation: null,
        axes: [axis({ chip_axis: "X" }), axis({ chip_axis: "X" }), axis({ chip_axis: "Z" })],
        quality: { status: "Error", messages: ["Same axis detected twice."] },
      }),
    ]);
    await openTab("still");
    expect(await summaryText()).toMatch(/no motion seen by this chip/i);
    expect(snippetText()).toBeNull();
    expect(screen.queryByRole("button", { name: "Copy" })).toBeNull();
    await openTab("twisted");
    expect(await summaryText()).toMatch(/unable to determine correctly/i);
    expect(snippetText()).toBeNull();
    expect(screen.queryByRole("button", { name: "Copy" })).toBeNull();
  });

  it("an_incomplete_chip_shows_what_is_missing_and_no_snippet", async () => {
    show(sessionMeta([chipMeta("ok"), chipMeta("short")]), [
      analysed("ok"),
      { state: "Incomplete", chip: "short", missing: "segment Z has no recording" },
    ]);
    const tab = await openTab("short");
    expect(tab.textContent).toMatch(/incomplete capture/i);
    expect(tab.textContent).toContain("segment Z has no recording");
    expect(snippetText()).toBeNull();
    expect(screen.queryByRole("button", { name: "Copy" })).toBeNull();
    // The other chip of the run keeps its snippet.
    await openTab("ok");
    expect(snippetText()).toBe("[ok]\naxes_map: x, -z, y\n");
  });

  it("the_configured_value_is_compared", async () => {
    show(sessionMeta([chipMeta("same"), chipMeta("other"), chipMeta("unset")]), [
      analysed("same", { configured_map: "x, -z, y", config_check: "already_correct" }),
      analysed("other", { configured_map: "x, y, z", config_check: "does_not_match" }),
      analysed("unset", { configured_map: null, config_check: "does_not_match" }),
    ]);
    await openTab("same");
    expect(await summaryText()).toContain("matches the detected map");
    expect(await summaryText()).toContain("No change needed");
    await openTab("other");
    expect(await summaryText()).toContain("differs from the detected map");
    expect(await summaryText()).not.toContain("No change needed");
    await openTab("unset");
    expect(await summaryText()).toContain("differs from the detected map");
  });

  it("an_unset_map_is_shown_and_compared_as_x_y_z", async () => {
    show(sessionMeta([chipMeta("plain")]), [
      analysed("plain", { map: "x, y, z", configured_map: null, config_check: "already_correct" }),
    ]);
    await openTab("plain");
    const text = await summaryText();
    expect(text).toMatch(/Configured map\s*x, y, z/);
    expect(text).toContain("matches the detected map");
    expect(text).not.toMatch(/none configured/i);
  });

  it("the_configured_value_is_shown_from_the_result", async () => {
    // The session's chip record holds another value. The page shows the one the analysis compared.
    show(sessionMeta([chipMeta("set", { axes_map: "z, y, x" }), chipMeta("unset", { axes_map: "z, y, x" })]), [
      analysed("set", { configured_map: "x, -y, -z", config_check: "does_not_match" }),
      analysed("unset", { configured_map: null, config_check: "does_not_match" }),
    ]);
    await openTab("set");
    const set = await summaryText();
    expect(set).toContain("x, -y, -z");
    expect(set).not.toContain("z, y, x");
    await openTab("unset");
    const unset = await summaryText();
    expect(unset).toContain("x, y, z");
    expect(unset).not.toContain("z, y, x");
  });

  it("the_detected_and_configured_maps_are_written_alike", async () => {
    show(sessionMeta([chipMeta("c")]), [
      analysed("c", { map: "x, -z, y", configured_map: "x,-z,y", config_check: "already_correct" }),
    ]);
    await openTab("c");
    const text = await summaryText();
    expect(text).toMatch(/Detected map\s*x, -z, y/);
    expect(text).toMatch(/Configured map\s*x, -z, y/);
    expect(text).not.toContain("x,-z,y");
  });

  it("a_good_detection_shows_a_status_and_no_callout_or_sentence", async () => {
    show(sessionMeta([chipMeta("c")]), [analysed("c")]);
    const tab = await openTab("c");
    expect(tab.textContent).toMatch(/Detection quality\s*Good/);
    expect(document.body.textContent).not.toContain("Detection quality: GOOD");
    const card = await screen.findByRole("region", { name: "Result" });
    expect(card.textContent).not.toContain("Detection quality:");
    expect(within(card).queryByRole("alert")).toBeNull();
  });

  it("a_warning_is_a_callout_in_the_result_card_with_one_line_per_finding", async () => {
    const angle = "High angle error detected (15.3 degrees)";
    const noise = "High noise level (800 mm/s^2)";
    show(sessionMeta([chipMeta("c")]), [analysed("c", { quality: { status: "Warning", messages: [angle, noise] } })]);
    const tab = await openTab("c");
    expect(tab.textContent).toMatch(/Detection quality\s*Warning/);
    const card = await screen.findByRole("region", { name: "Result" });
    const callout = within(card).getByText("Detection quality: Warning").closest('[role="status"]') as HTMLElement;
    expect(callout).toBeTruthy();
    expect(within(callout).getByText(angle).tagName).toBe("P");
    expect(within(callout).getByText(noise).tagName).toBe("P");
    expect(card.querySelector("ul")).toBeNull();
    expect(tab.textContent).not.toContain(angle);
  });

  it("an_error_is_announced_as_an_alert", async () => {
    show(sessionMeta([chipMeta("c")]), [
      analysed("c", {
        state: "ambiguous",
        map: null,
        config_check: null,
        euler_deg: null,
        rotation: null,
        quality: { status: "Error", messages: ["Same axis detected twice."] },
      }),
    ]);
    await openTab("c");
    const card = await screen.findByRole("region", { name: "Result" });
    const alert = within(card).getByRole("alert");
    expect(alert.textContent).toContain("Detection quality: Error");
    expect(alert.textContent).toContain("Unable to determine correctly");
    expect(alert.textContent).toContain("Same axis detected twice.");
  });

  it("an_axis_without_signal_is_called_inferred", async () => {
    show(sessionMeta([chipMeta("c")]), [
      analysed("c", {
        axes: [
          axis({ chip_axis: "X" }),
          axis({ chip_axis: "Z", negative: true }),
          axis({ state: "virtual", chip_axis: "Y", confidence: 0, tilt_deg: 0 }),
        ],
      }),
    ]);
    await openTab("c");
    const rows = (await screen.findAllByRole("row")).map((r) => r.textContent);
    expect(rows.find((r) => r?.startsWith("Z"))).toContain("Inferred");
    expect(document.body.textContent).not.toMatch(/virtual/i);
  });

  it("the_page_shows_neither_euler_angles_nor_gravity", async () => {
    show(sessionMeta([chipMeta("c")]), [analysed("c", { euler_deg: [90.9, -1.4, 5.7] })]);
    const tab = await openTab("c");
    expect(tab.textContent).toContain("Noise");
    expect(document.body.textContent).not.toMatch(/euler|roll|pitch|yaw|gravity/i);
  });

  it("every_field_is_in_one_grid_and_the_health_notes_are_below_it", async () => {
    show(sessionMeta([chipMeta("c")]), [analysed("c", {}, "Degraded", ["segment Y: 12 samples dropped"])]);
    const tab = await openTab("c");
    const grid = tab.querySelector("dl") as HTMLElement;
    const labels = Array.from(grid.querySelectorAll("dt")).map((dt) => dt.textContent);
    expect(labels).toEqual(["Mounted on", "Type", "Capture health", "Detection quality", "Noise"]);
    const notes = within(tab).getByRole("list", { name: "Capture health notes" });
    expect(notes.textContent).toContain("segment Y: 12 samples dropped");
    expect(grid.contains(notes)).toBe(false);
  });

  it("the_mounting_is_written_in_title_case", async () => {
    show(sessionMeta([chipMeta("c", { mounting: "toolhead" }), chipMeta("d", { mounting: "bed" })]), [
      analysed("c"),
      analysed("d"),
    ]);
    expect((await openTab("c")).textContent).toMatch(/Mounted on\s*Toolhead/);
    expect((await openTab("d")).textContent).toMatch(/Mounted on\s*Bed/);
  });

  it("the_result_card_holds_the_chip_tabs_and_the_detected_map", async () => {
    show(sessionMeta([chipMeta("a"), chipMeta("b")]), [analysed("a"), analysed("b")]);
    const card = await screen.findByRole("region", { name: "Result" });
    expect(card.querySelectorAll('[role="tab"]')).toHaveLength(2);
    expect(card.textContent).toContain("Detected map");
    expect(card.textContent).toContain("x, -z, y");
  });

  it("a_degraded_complete_capture_shows_its_health_notes_and_keeps_the_snippet", async () => {
    show(sessionMeta([chipMeta("adxl345")]), [analysed("adxl345", {}, "Degraded", ["segment Y: 12 samples dropped"])]);
    const tab = await openTab("adxl345");
    expect(tab.textContent).toContain("Degraded");
    expect(tab.textContent).toContain("segment Y: 12 samples dropped");
    expect(snippetText()).toBe("[adxl345]\naxes_map: x, -z, y\n");
    expect(screen.getByRole("button", { name: "Copy" })).toBeTruthy();
  });

  it("a_no_motion_result_shows_the_unknown_z_limit_note", async () => {
    show(sessionMeta([chipMeta("lis2dw bed", { chip_type: "lis2dw", mounting: "bed" })]), [noMotion("lis2dw bed")]);
    await openTab("lis2dw bed");
    expect(await summaryText()).toContain(NO_Z_LIMITS);
  });

  it("the_mounting_is_shown_for_each_chip", async () => {
    show(sessionMeta([chipMeta("on bed", { mounting: "bed" }), chipMeta("old", { mounting: null })]), [
      analysed("on bed"),
      { state: "Failed", chip: "old", message: "segment X: not enough samples" },
    ]);
    expect((await openTab("on bed")).textContent).toMatch(/Mounted on\s*Bed/);
    const old = await openTab("old");
    // A session from before mountings were recorded was analysed as a toolhead chip.
    expect(old.textContent).toMatch(/Mounted on\s*Toolhead/);
    expect(old.textContent).toContain("segment X: not enough samples");
  });

  it("a_beacon_chip_gets_the_accel_axes_map_snippet_through_the_page", async () => {
    show(sessionMeta([chipMeta("beacon", { chip_type: "beacon" })]), [analysed("beacon")]);
    await openTab("beacon");
    expect(snippetText()).toBe("[beacon]\naccel_axes_map: x, -z, y\n");
  });

  it("the_header_is_the_section_name_verbatim", async () => {
    show(
      sessionMeta([
        chipMeta("adxl345 nh36"),
        chipMeta("lis2dw bed", { chip_type: "lis2dw" }),
        chipMeta("beacon", { chip_type: "beacon" }),
      ]),
      [analysed("adxl345 nh36"), analysed("lis2dw bed"), analysed("beacon")],
    );
    await openTab("adxl345 nh36");
    expect(snippetText()).toBe("[adxl345 nh36]\naxes_map: x, -z, y\n");
    await openTab("lis2dw bed");
    expect(snippetText()).toBe("[lis2dw bed]\naxes_map: x, -z, y\n");
    await openTab("beacon");
    expect(snippetText()).toBe("[beacon]\naccel_axes_map: x, -z, y\n");
  });
});
