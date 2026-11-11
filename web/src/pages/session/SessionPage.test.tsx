import { fireEvent, screen, waitFor, within } from "@testing-library/react";
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

/** `perChip` holds the snippet of each chip's recommended shaper, which every candidate row repeats. */
function result(a: Analysis, perChip: (string | null)[]): AnalysisResult {
  return {
    analysis: a,
    snippets: {
      worstCase: a.worst_case ? SNIPPET : null,
      perChip: perChip.map((snippet, i) =>
        snippet === null ? null : (a.per_chip[i]?.recommendation?.all.map(() => snippet) ?? [snippet]),
      ),
    },
  };
}

function serve(meta = twoAxisMeta()) {
  return stubApi(({ url }) =>
    url.pathname.endsWith("/sessions/s1")
      ? { body: { summary: summary("s1"), meta } }
      : { status: 404, body: { error: "not_found", message: "no such session" } },
  );
}

/** Opens a chip's tab and returns its card; with several chips the page shows one at a time. */
async function chipCard(name: string) {
  fireEvent.click(await screen.findByRole("tab", { name }));
  return screen.findByRole("region", { name: `Chip ${name}` });
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
    expect(await screen.findByText(/no record of whether samples were dropped/)).toBeTruthy();
    expect(screen.getAllByText(/shaper_type_x: mzv/).length).toBeGreaterThan(0);
    const worst = screen.getByRole("region", { name: "Recommended Input Shaper" });
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
    const worst = await screen.findByRole("region", { name: "Recommended Input Shaper" });
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
    const degraded = await chipCard("b");
    expect(degraded.textContent).toContain("No recommendation for this chip: degraded capture health");
    expect(degraded.querySelector('[role="img"]')).toBeTruthy();
    expect(screen.getByRole("region", { name: "Recommended Input Shaper" }).textContent).toContain("mzv at 48.2 Hz");
  });

  it("lets the user include an excluded chip, and analyses again with it named", async () => {
    const a = analysis();
    a.eligibility.decisions = [decision("a", "Included"), decision("b", "Excluded", ["degraded capture health"])];
    fake.analyze.mockResolvedValue(result(a, [SNIPPET, null]));
    serve();
    renderRoute("/sessions/s1");
    const box = await screen.findByRole("checkbox", { name: /Include b anyway/ });
    expect(box.getAttribute("aria-checked")).toBe("false");
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
    expect(box.getAttribute("aria-checked")).toBe("true");
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
    expect(screen.getAllByText(/The capture is incomplete: tail/).length).toBeGreaterThan(0);
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
    const panel = await screen.findByRole("region", { name: "Data Quality" });
    expect(panel.textContent).toContain("Could not be analysed: timestamps are not finite and strictly increasing");
    expect((await chipCard("b")).textContent).toContain(
      "No recommendation for this chip: timestamps are not finite and strictly increasing",
    );
    expect(screen.getByRole("region", { name: "Recommended Input Shaper" })).toBeTruthy();
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
    expect((await chipCard("b")).querySelector('[role="img"]')).toBeTruthy();
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
    expect(screen.getByRole("region", { name: "Data Quality" }).textContent).toContain("395 Hz");
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
      expect(screen.getByRole("region", { name: "Data Quality" }).textContent).toContain(
        "Could not be analysed: flat signal",
      ),
    );
    expect((await chipCard("b")).textContent).toContain(
      "No recommendation for this chip: flat signal: no vibration power to analyse",
    );
    const box = screen.getByRole("checkbox", { name: /Include b anyway/ });
    expect(box.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(box);
    // Back to the earlier key, whose result is cached, so there is no new analysis to wait for.
    await waitFor(() =>
      expect(screen.getByRole("checkbox", { name: /Include b anyway/ }).getAttribute("aria-checked")).toBe("false"),
    );
  });

  it("judges the data on this axis, not the chip\u2019s health over the whole test", async () => {
    const a = analysis();
    a.per_chip = [{ ...a.per_chip[0], chip: "adxl345", health: "Ok" }];
    a.eligibility.decisions = [decision("adxl345", "Included")];
    fake.analyze.mockResolvedValue(result(a, [SNIPPET]));
    const meta = twoAxisMeta();
    meta.chips = meta.chips.map((c) => ({ ...c, health: "Degraded" }));
    serve(meta);
    renderRoute("/sessions/s1");
    const quality = await screen.findByRole("region", { name: "Data Quality" });
    expect(quality.textContent).toContain("Good data");
    expect(quality.textContent).not.toContain("Left out");
  });

  it("shows one chip at a time, with a tab for each chip", async () => {
    fake.analyze.mockResolvedValue(result(analysis(), [SNIPPET, SNIPPET]));
    serve();
    renderRoute("/sessions/s1");
    const first = await screen.findByRole("region", { name: "Chip a" });
    expect(first).toBeTruthy();
    expect(screen.queryByRole("region", { name: "Chip b" })).toBeNull();
    fireEvent.click(screen.getByRole("tab", { name: "b" }));
    expect(await screen.findByRole("region", { name: "Chip b" })).toBeTruthy();
    expect(screen.queryByRole("region", { name: "Chip a" })).toBeNull();
  });

  it("shows the tab even for a single chip, so the page looks the same", async () => {
    const a = analysis();
    a.per_chip = [{ ...a.per_chip[0], chip: "adxl345" }];
    a.eligibility.decisions = [decision("adxl345", "Included")];
    fake.analyze.mockResolvedValue(result(a, [SNIPPET]));
    serve();
    renderRoute("/sessions/s1");
    await screen.findByRole("region", { name: "Chip adxl345" });
    expect(screen.getByRole("tab", { name: "adxl345" })).toBeTruthy();
  });

  it("expands the chart and table in a dialog and closes it again", async () => {
    fake.analyze.mockResolvedValue(result(analysis(), [SNIPPET, SNIPPET]));
    serve();
    renderRoute("/sessions/s1");
    const card = await screen.findByRole("region", { name: "Chip a" });
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(within(card).getByRole("button", { name: "Expand the chart and table" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/Resonance and Shapers: a/)).toBeTruthy();
    expect(within(dialog).getByRole("img", { name: /PSD of a/ })).toBeTruthy();
    expect(within(dialog).getAllByRole("row").length).toBeGreaterThan(1);
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("has an All Chips tab, last, when there are several chips, and it explains what it shows", async () => {
    fake.analyze.mockResolvedValue(result(analysis(), [SNIPPET, SNIPPET]));
    serve();
    renderRoute("/sessions/s1");
    const tabs = await screen.findAllByRole("tab", { name: /^(a|b|All Chips)$/ });
    expect(tabs.map((t) => t.textContent)).toEqual(["a", "b", "All Chips"]);
    fireEvent.click(tabs[2]);
    const all = await screen.findByRole("region", { name: "All chips" });
    expect(all.textContent).toContain("adds the chips together");
    expect(all.textContent).toContain("only for looking");
    expect(screen.queryByRole("region", { name: "Chip a" })).toBeNull();
    // Back to a chip.
    fireEvent.click(screen.getByRole("tab", { name: "a" }));
    expect(await screen.findByRole("region", { name: "Chip a" })).toBeTruthy();
  });

  it("has no All Chips tab for a single chip", async () => {
    const a = analysis();
    a.per_chip = [{ ...a.per_chip[0], chip: "adxl345" }];
    a.eligibility.decisions = [decision("adxl345", "Included")];
    fake.analyze.mockResolvedValue(result(a, [SNIPPET]));
    serve();
    renderRoute("/sessions/s1");
    await screen.findByRole("region", { name: "Chip adxl345" });
    expect(screen.queryByRole("tab", { name: "All Chips" })).toBeNull();
  });

  it("analyses with the printer's recorded corner velocity and damping, until the user applies others", async () => {
    fake.analyze.mockResolvedValue(result(analysis(), [SNIPPET, SNIPPET]));
    const meta = twoAxisMeta();
    meta.printer = {
      name: null,
      klipper_version: null,
      mcu_firmware: [],
      square_corner_velocity: 8,
      damping_ratio_x: 0.12,
      damping_ratio_y: null,
    };
    serve(meta);
    renderRoute("/sessions/s1");
    await screen.findByRole("region", { name: "Recommended Input Shaper" });
    expect(fake.analyze.mock.calls[0][0].params).toMatchObject({ scv: 8, damping_ratio: 0.12 });
    fireEvent.change(screen.getByLabelText(/Square Corner Velocity/), { target: { value: "3" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(fake.analyze.mock.calls.at(-1)?.[0].params).toMatchObject({ scv: 3 }));
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
    await screen.findByRole("region", { name: "Recommended Input Shaper" });
    fireEvent.change(screen.getByLabelText(/Maximum Corner Rounding/), { target: { value: "0.1" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(fake.analyze).toHaveBeenCalledTimes(2));
    expect(fake.analyze.mock.calls[1][0].params.max_smoothing).toBe(0.1);
  });

  it("shows how the session was analysed in its details, and no longer a line of jargon", async () => {
    const a = analysis();
    a.worst_case = recommendation();
    fake.analyze.mockResolvedValue(result(a, [SNIPPET, null]));
    serve();
    renderRoute("/sessions/s1");
    const details = await screen.findByRole("region", { name: "Session details" });
    await waitFor(() => expect(details.textContent).toContain("Analysis Version"));
    expect(details.textContent).toContain("0.1.0");
    expect(details.textContent).toContain("Shapers Tried");
    expect(details.textContent).toContain("200 Hz");
    expect(screen.queryByText(/Analysed with core/)).toBeNull();
  });

  it("shows the daemon\u2019s message when the session cannot be loaded", async () => {
    serve();
    renderRoute("/sessions/other");
    expect(await screen.findByText("no such session")).toBeTruthy();
    expect(fake.analyze).not.toHaveBeenCalled();
  });
});
