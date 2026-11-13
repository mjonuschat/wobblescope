import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createAnalysisClient, type WorkerLike } from "@/analysis/client";
import { createHandler } from "@/analysis/handler";
import type { WorkerResponse } from "@/analysis/protocol";
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
    config_check: "none_configured",
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
  fireEvent.click(await screen.findByRole("tab", { name }));
  return screen.findByRole("region", { name: `Chip ${name}` });
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
    expect((await screen.findAllByRole("tab")).map((t) => t.textContent)).toEqual(["a", "b"]);
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
    const tab = await openTab("adxl345 bed");
    const rows = (await screen.findAllByRole("row")).map((r) => r.textContent);
    expect(rows.find((r) => r?.startsWith("Y"))).toContain("-y");
    expect(rows.find((r) => r?.startsWith("X"))).toContain("not determined");
    expect(rows.find((r) => r?.startsWith("Z"))).toContain("not determined");
    expect(tab.textContent).toContain("Only the Y axis was seen moving");
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
    expect((await openTab("still")).textContent).toMatch(/no motion seen by this chip/i);
    expect(snippetText()).toBeNull();
    expect(screen.queryByRole("button", { name: "Copy" })).toBeNull();
    expect((await openTab("twisted")).textContent).toMatch(/unable to determine correctly/i);
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
      analysed("unset", { configured_map: null, config_check: "none_configured" }),
    ]);
    expect((await openTab("same")).textContent).toContain("already correct");
    expect((await openTab("other")).textContent).toContain("does not match");
    expect((await openTab("unset")).textContent).toContain("none configured");
  });

  it("the_configured_value_is_shown_from_the_result", async () => {
    // The session's chip record holds another value. The page shows the one the analysis compared.
    show(sessionMeta([chipMeta("set", { axes_map: "z, y, x" }), chipMeta("unset", { axes_map: "z, y, x" })]), [
      analysed("set", { configured_map: "x, -y, -z", config_check: "does_not_match" }),
      analysed("unset", { configured_map: null, config_check: "none_configured" }),
    ]);
    const set = await openTab("set");
    expect(set.textContent).toContain("x, -y, -z");
    expect(set.textContent).not.toContain("z, y, x");
    const unset = await openTab("unset");
    expect(unset.textContent).toContain("none configured");
    expect(unset.textContent).not.toContain("z, y, x");
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
    const tab = await openTab("lis2dw bed");
    expect(tab.textContent).toContain(NO_Z_LIMITS);
  });

  it("the_mounting_is_shown_for_each_chip", async () => {
    show(sessionMeta([chipMeta("on bed", { mounting: "bed" }), chipMeta("old", { mounting: null })]), [
      analysed("on bed"),
      { state: "Failed", chip: "old", message: "segment X: not enough samples" },
    ]);
    expect((await openTab("on bed")).textContent).toMatch(/mounted on\s*bed/i);
    const old = await openTab("old");
    // A session from before mountings were recorded was analysed as a toolhead chip.
    expect(old.textContent).toMatch(/mounted on\s*toolhead/i);
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
