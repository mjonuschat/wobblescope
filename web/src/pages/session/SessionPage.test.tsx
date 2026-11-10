import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AnalysisResult } from "@/analysis/protocol";
import type { Analysis } from "@/lib/generated/Analysis";
import { analysis, decision, recommendation, twoAxisMeta } from "@/test/fixtures";
import { stubApiWithStatus as stubApi } from "@/test/status";
import { renderRoute } from "@/test/render";
import { summary } from "@/test/summaries";

const fake = vi.hoisted(() => ({ analyze: vi.fn(), forget: vi.fn() }));

vi.mock("@/analysis/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/analysis/client")>()),
  getAnalysisClient: () => fake,
}));
vi.mock("@/components/charts/EChart", () => ({
  EChart: ({ label }: { label: string }) => <div role="img" aria-label={label} />,
}));

const SNIPPET = "[input_shaper]\nshaper_type_x: mzv\nshaper_freq_x: 48.2\ndamping_ratio_x: 0.1\n";

function result(a: Analysis, perChip: (string | null)[]): AnalysisResult {
  return { analysis: a, snippets: { worstCase: a.worst_case ? SNIPPET : null, perChip } };
}

function serve(meta = twoAxisMeta()) {
  return stubApi(({ url }) =>
    url.pathname.endsWith("/sessions/s1")
      ? { body: { summary: summary("s1"), meta } }
      : { status: 404, body: { error: "not_found", message: "no such session" } },
  );
}

function clipboard(value: unknown) {
  Object.defineProperty(navigator, "clipboard", { value, configurable: true });
}

beforeEach(() => {
  fake.analyze.mockReset();
});
afterEach(() => clipboard(undefined));

describe("the session view", () => {
  it("covers AE3: an imported session shows the unknown-health warning and a copyable snippet", async () => {
    const a = analysis();
    a.per_chip = [{ ...a.per_chip[0], chip: "adxl345" }];
    a.eligibility.decisions = [decision("adxl345", "Included", ["unknown capture health"])];
    fake.analyze.mockResolvedValue(result(a, [SNIPPET]));
    const writeText = vi.fn().mockResolvedValue(undefined);
    clipboard({ writeText });
    serve();
    renderRoute("/sessions/s1");
    expect(await screen.findByText(/unknown capture health/)).toBeTruthy();
    expect(screen.getAllByText(/shaper_type_x: mzv/).length).toBeGreaterThan(0);
    const worst = screen.getByRole("region", { name: "Worst case across all chips" });
    fireEvent.click(worst.querySelector("button")!);
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(SNIPPET));
    expect(await screen.findByText("Copied")).toBeTruthy();
  });

  it.each([
    ["no clipboard", undefined],
    ["a rejecting clipboard", { writeText: () => Promise.reject(new Error("denied")) }],
  ])("says the copy failed and keeps the text on screen with %s", async (_name, value) => {
    fake.analyze.mockResolvedValue(result(analysis(), [SNIPPET, null]));
    clipboard(value);
    serve();
    renderRoute("/sessions/s1");
    const worst = await screen.findByRole("region", { name: "Worst case across all chips" });
    fireEvent.click(worst.querySelector("button")!);
    expect(await screen.findByText(/Copy failed/)).toBeTruthy();
    expect(worst.textContent).toContain("shaper_type_x: mzv");
  });

  it("covers AE4: a degraded chip is drawn, has no recommendation, and the worst case comes from the other chip", async () => {
    const a = analysis();
    a.eligibility.decisions = [decision("a", "Included"), decision("b", "Excluded", ["degraded capture health"])];
    fake.analyze.mockResolvedValue(result(a, [SNIPPET, null]));
    serve();
    renderRoute("/sessions/s1");
    const degraded = await screen.findByRole("region", { name: "Chip b" });
    expect(degraded.textContent).toContain("No recommendation for this chip: degraded capture health");
    expect(degraded.querySelector('[role="img"]')).toBeTruthy();
    expect(screen.getByRole("region", { name: "Worst case across all chips" }).textContent).toContain("mzv at 48.2 Hz");
  });

  it("lets the user include an excluded chip, and analyses again with it named", async () => {
    const a = analysis();
    a.eligibility.decisions = [decision("a", "Included"), decision("b", "Excluded", ["degraded capture health"])];
    fake.analyze.mockResolvedValue(result(a, [SNIPPET, null]));
    serve();
    renderRoute("/sessions/s1");
    const box = await screen.findByRole("checkbox", { name: /Include b anyway/ });
    expect((box as HTMLInputElement).checked).toBe(false);
    expect(screen.queryByRole("checkbox", { name: /Include a anyway/ })).toBeNull();
    expect(fake.analyze.mock.calls[0][0].overrides).toEqual([]);
    fireEvent.click(box);
    await waitFor(() => expect(fake.analyze).toHaveBeenCalledTimes(2));
    expect(fake.analyze.mock.calls[1][0].overrides).toEqual(["b"]);
  });

  it("shows a chip that was included by override as checked", async () => {
    const a = analysis();
    a.eligibility.decisions = [decision("a", "Included"), decision("b", "Overridden", ["degraded capture health"])];
    fake.analyze.mockResolvedValue(result(a, [SNIPPET, SNIPPET]));
    serve();
    renderRoute("/sessions/s1");
    const box = await screen.findByRole("checkbox", { name: /Include b anyway/ });
    expect((box as HTMLInputElement).checked).toBe(true);
  });

  it("explains an axis without any eligible dataset and shows no snippet", async () => {
    const a = analysis();
    a.worst_case = null;
    a.per_chip = a.per_chip.map((c) => ({ ...c, recommendation: null }));
    a.eligibility.decisions = [
      decision("a", "Excluded", ["degraded capture health"]),
      decision("b", "Excluded", ["incomplete coverage: tail"]),
    ];
    fake.analyze.mockResolvedValue(result(a, [null, null]));
    serve();
    renderRoute("/sessions/s1");
    expect(await screen.findByText(/none of its recordings could be used/)).toBeTruthy();
    expect(screen.getAllByText(/incomplete coverage: tail/).length).toBeGreaterThan(0);
    expect(screen.queryByText(/shaper_type_x/)).toBeNull();
  });

  it("covers AE1: a dropped recording is a warning and the other chip\u2019s results stay", async () => {
    const a = analysis();
    a.eligibility.decisions = [
      decision("a", "Included"),
      decision("b", "Included", [], "timestamps are not finite and strictly increasing"),
    ];
    fake.analyze.mockResolvedValue(result(a, [SNIPPET, null]));
    serve();
    renderRoute("/sessions/s1");
    const panel = await screen.findByRole("region", { name: "Capture health and eligibility" });
    expect(panel.textContent).toContain(
      "included, but could not be analysed: timestamps are not finite and strictly increasing",
    );
    expect(screen.getByRole("region", { name: "Chip b" }).textContent).toContain(
      "No recommendation for this chip: timestamps are not finite and strictly increasing",
    );
    expect(screen.getByRole("region", { name: "Worst case across all chips" })).toBeTruthy();
    expect(screen.queryByRole("checkbox", { name: /Include b anyway/ })).toBeNull();
  });

  it("covers AE2: when the only eligible recording is dropped the axis has no recommendation and says why", async () => {
    const a = analysis();
    a.worst_case = null;
    a.summed = null;
    a.per_chip = a.per_chip.map((c) => ({ ...c, recommendation: null }));
    a.eligibility.decisions = [
      decision("a", "Included", [], "too few samples: have 0, need more than 1"),
      decision("b", "Excluded", ["degraded capture health"]),
    ];
    fake.analyze.mockResolvedValue(result(a, [null, null]));
    serve();
    renderRoute("/sessions/s1");
    expect(await screen.findByText(/none of its recordings could be used/)).toBeTruthy();
    expect(screen.getAllByText(/too few samples: have 0/).length).toBeGreaterThan(0);
    expect(screen.getByRole("region", { name: "Chip b" }).querySelector('[role="img"]')).toBeTruthy();
  });

  it("covers AE6: an undersampled chip can be included anyway", async () => {
    const reason =
      "samples arrive at 395 Hz, under half the 1600 Hz Klipper configures; check that the chip's MCU runs current firmware";
    const a = analysis();
    a.eligibility.decisions = [decision("a", "Included"), decision("b", "Excluded", [reason])];
    fake.analyze.mockResolvedValue(result(a, [SNIPPET, null]));
    serve();
    renderRoute("/sessions/s1");
    const box = await screen.findByRole("checkbox", { name: /Include b anyway/ });
    expect(box.closest("label")!.textContent).toContain("395 Hz");
    fireEvent.click(box);
    await waitFor(() => expect(fake.analyze).toHaveBeenCalledTimes(2));
    expect(fake.analyze.mock.calls[1][0].overrides).toEqual(["b"]);
  });

  it("keeps the override box checked for an overridden chip that was dropped, and unticking removes it", async () => {
    const excluded = analysis();
    excluded.eligibility.decisions = [
      decision("a", "Included"),
      decision("b", "Excluded", ["degraded capture health"]),
    ];
    const dropped = analysis();
    dropped.eligibility.decisions = [
      decision("a", "Included"),
      decision("b", "Overridden", ["degraded capture health"], "flat signal: no vibration power to analyse"),
    ];
    fake.analyze.mockImplementation(async (job: { overrides: string[] }) =>
      result(job.overrides.includes("b") ? dropped : excluded, [SNIPPET, null]),
    );
    serve();
    renderRoute("/sessions/s1");
    fireEvent.click(await screen.findByRole("checkbox", { name: /Include b anyway/ }));
    await waitFor(() =>
      expect(screen.getByRole("region", { name: "Capture health and eligibility" }).textContent).toContain(
        "included by override (degraded capture health), but could not be analysed: flat signal",
      ),
    );
    expect(screen.getByRole("region", { name: "Chip b" }).textContent).toContain(
      "No recommendation for this chip: flat signal: no vibration power to analyse",
    );
    const box = screen.getByRole("checkbox", { name: /Include b anyway/ });
    expect((box as HTMLInputElement).checked).toBe(true);
    fireEvent.click(box);
    // Back to the earlier key, whose result is cached, so there is no new analysis to wait for.
    await waitFor(() =>
      expect((screen.getByRole("checkbox", { name: /Include b anyway/ }) as HTMLInputElement).checked).toBe(false),
    );
  });

  it("shows the chip\u2019s health on this axis, not its health over the whole test", async () => {
    const a = analysis();
    a.per_chip = [{ ...a.per_chip[0], chip: "adxl345", health: "Ok" }];
    a.eligibility.decisions = [decision("adxl345", "Included")];
    fake.analyze.mockResolvedValue(result(a, [SNIPPET]));
    const meta = twoAxisMeta();
    meta.chips = meta.chips.map((c) => ({ ...c, health: "Degraded" }));
    serve(meta);
    renderRoute("/sessions/s1");
    const chip = await screen.findByRole("region", { name: "Chip adxl345" });
    expect(chip.textContent).toContain("Ok");
    expect(chip.textContent).not.toContain("Degraded");
  });

  it("shows an analysis error while the metadata stays visible", async () => {
    fake.analyze.mockImplementation(() => Promise.reject(new Error("c0-s0.zst holds 999 samples, metadata says 1000")));
    serve();
    renderRoute("/sessions/s1");
    expect(await screen.findByText(/c0-s0.zst holds 999 samples, metadata says 1000/)).toBeTruthy();
    expect(screen.getByRole("region", { name: "Session details" })).toBeTruthy();
  });

  it("shows only the axes the session recorded", async () => {
    fake.analyze.mockResolvedValue(result(analysis(), [SNIPPET, null]));
    const meta = twoAxisMeta();
    meta.segments = meta.segments.filter((s) => s.axis === "X");
    serve(meta);
    renderRoute("/sessions/s1");
    await screen.findByRole("tab", { name: "Axis X" });
    expect(screen.queryByRole("tab", { name: "Axis Y" })).toBeNull();
  });

  it("analyses the other axis when its tab is chosen", async () => {
    fake.analyze.mockResolvedValue(result(analysis(), [SNIPPET, null]));
    serve();
    renderRoute("/sessions/s1");
    fireEvent.click(await screen.findByRole("tab", { name: "Axis Y" }));
    await waitFor(() => expect(fake.analyze.mock.calls.map((c) => c[0].axis)).toEqual(["X", "Y"]));
  });

  it("re-runs with the fit settings that are applied", async () => {
    fake.analyze.mockResolvedValue(result(analysis(), [SNIPPET, null]));
    serve();
    renderRoute("/sessions/s1");
    await screen.findByRole("region", { name: "Worst case across all chips" });
    fireEvent.change(screen.getByLabelText(/Smoothing limit/), { target: { value: "0.1" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(fake.analyze).toHaveBeenCalledTimes(2));
    expect(fake.analyze.mock.calls[1][0].params.max_smoothing).toBe(0.1);
  });

  it("shows the core version and the effective fit settings", async () => {
    const a = analysis();
    a.worst_case = recommendation();
    fake.analyze.mockResolvedValue(result(a, [SNIPPET, null]));
    serve();
    renderRoute("/sessions/s1");
    const note = await screen.findByText(/Analysed with core 0.1.0/);
    expect(note.textContent).toContain("corner velocity 5 mm/s");
    expect(note.textContent).toContain("up to 200 Hz");
  });

  it("shows the daemon\u2019s message when the session cannot be loaded", async () => {
    serve();
    renderRoute("/sessions/other");
    expect(await screen.findByText("no such session")).toBeTruthy();
    expect(fake.analyze).not.toHaveBeenCalled();
  });
});
