import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { FinishedTest } from "@/lib/generated/FinishedTest";
import type { TestKind } from "@/lib/generated/TestKind";
import type { TestState } from "@/lib/generated/TestState";
import { chipsSnapshot, printerSettings } from "@/test/capture";
import type { ApiCall, Reply } from "@/test/http";
import { renderRoute } from "@/test/render";
import { apiPath, stubApiWithStatus } from "@/test/status";
import { FakeSocket, stubWebSocket } from "@/test/websocket";

vi.mock("@/components/charts/EChart", () => ({
  EChart: ({ label }: { label: string }) => <div role="img" aria-label={label} />,
}));

const RESTART_INSTRUCTION =
  "The calibration may have left changed acceleration limits or input shaping. Send RESTART from the Klipper console. The printer needs homing afterwards, and any positioning mode or offsets you rely on must be set again.";

function api(start: Reply = { status: 202, body: { id: "run-1" } }) {
  return stubApiWithStatus((call: ApiCall) => {
    const path = apiPath(call);
    if (path === "settings") return { body: printerSettings() };
    if (path === "chips") return { body: chipsSnapshot() };
    if (path === "printer") return { body: { homed_axes: "xyz" } };
    if (path === "axes-map" && call.method === "POST") return start;
    return { status: 404, body: { error: "not_found", message: "no" } };
  });
}

const posted = (calls: ApiCall[]) => calls.filter((c) => c.method === "POST" && apiPath(c) === "axes-map");
const field = (label: RegExp | string) => screen.getByLabelText(label) as HTMLInputElement;
const startButton = () =>
  screen.getByRole("button", { name: /Start calibration|Checking printer/ }) as HTMLButtonElement;

async function open(start?: Reply) {
  const calls = api(start);
  renderRoute("/run/axes-map");
  await screen.findByRole("button", { name: "Start calibration" });
  await waitFor(() => expect(startButton().disabled).toBe(false));
  return calls;
}

async function chooseMounting(chip: string, name: "Toolhead" | "Bed") {
  fireEvent.click(screen.getByRole("combobox", { name: `Mounted on ${chip}` }));
  const option = await screen.findByRole("option", { name });
  fireEvent.pointerDown(option);
  fireEvent.pointerUp(option);
  fireEvent.click(option);
}

const finished = (over: Partial<FinishedTest> = {}): FinishedTest => ({
  id: "run-1",
  kind: "axes_map",
  outcome: "complete",
  reason: null,
  code: null,
  session_id: "run-1",
  motion_uncertain: false,
  needs_restart: false,
  ...over,
});

async function openWithFinished(last: FinishedTest) {
  api();
  renderRoute("/run/axes-map");
  await waitFor(() => expect(FakeSocket.instances.length).toBeGreaterThan(0));
  act(() => {
    FakeSocket.latest.open();
    FakeSocket.latest.send({ test: null, last });
  });
}

describe("the axes-map run page", () => {
  beforeEach(() => stubWebSocket({ idle: true }));

  it("submits_chips_mounting_and_overrides", async () => {
    const calls = await open();
    expect(field(/Z height/).value).toBe("20");
    expect(field(/^Speed/).value).toBe("80");
    expect(field(/Acceleration/).value).toBe("1500");
    expect(field(/Travel speed/).value).toBe("120");
    expect(screen.getByText("A chip mounted under the nozzle needs a higher Z height")).toBeTruthy();
    fireEvent.change(field(/Z height/), { target: { value: "40" } });
    fireEvent.change(field(/Acceleration/), { target: { value: "2000" } });
    fireEvent.click(startButton());
    await waitFor(() => expect(posted(calls)).toHaveLength(1));
    expect(JSON.parse(String(posted(calls)[0].body))).toEqual({
      chips: [{ chip: "lis2dw ebb36", mounting: "toolhead" }],
      z_height: 40,
      accel: 2000,
    });
  });

  it("sends only the chips when no field was changed", async () => {
    const calls = await open();
    fireEvent.click(startButton());
    await waitFor(() => expect(posted(calls)).toHaveLength(1));
    expect(JSON.parse(String(posted(calls)[0].body))).toEqual({
      chips: [{ chip: "lis2dw ebb36", mounting: "toolhead" }],
    });
  });

  it("minimums_are_enforced_in_the_form", async () => {
    const calls = await open();
    fireEvent.change(field(/^Speed/), { target: { value: "10" } });
    expect(screen.getByText("Speed must be at least 20 mm/s.")).toBeTruthy();
    expect(startButton().disabled).toBe(true);
    fireEvent.change(field(/^Speed/), { target: { value: "20" } });
    fireEvent.change(field(/Acceleration/), { target: { value: "99" } });
    expect(screen.getByText("Acceleration must be at least 100 mm/s².")).toBeTruthy();
    fireEvent.change(field(/Acceleration/), { target: { value: "100" } });
    fireEvent.change(field(/Travel speed/), { target: { value: "19" } });
    expect(screen.getByText("Travel speed must be at least 20 mm/s.")).toBeTruthy();
    expect(startButton().disabled).toBe(true);
    fireEvent.submit(startButton().closest("form")!);
    expect(posted(calls)).toHaveLength(0);
    fireEvent.change(field(/Travel speed/), { target: { value: "20" } });
    expect(startButton().disabled).toBe(false);
  });

  it("bed_mounting_is_sent_for_a_bed_chip", async () => {
    const calls = await open();
    fireEvent.click(screen.getByRole("checkbox", { name: "adxl345" }));
    await chooseMounting("adxl345", "Bed");
    fireEvent.click(startButton());
    await waitFor(() => expect(posted(calls)).toHaveLength(1));
    expect(JSON.parse(String(posted(calls)[0].body)).chips).toEqual([
      { chip: "adxl345", mounting: "bed" },
      { chip: "lis2dw ebb36", mounting: "toolhead" },
    ]);
  });

  it("refuses to start with no chip chosen", async () => {
    await open();
    fireEvent.click(screen.getByRole("checkbox", { name: "lis2dw ebb36" }));
    expect(screen.getByText("Choose at least one chip.")).toBeTruthy();
    expect(startButton().disabled).toBe(true);
  });

  it("lists a refused start, including a restart owed", async () => {
    await open({
      status: 409,
      body: {
        error: "preflight_failed",
        message: "1 check failed",
        failures: [{ check: "restart_owed", message: "a calibration left Klipper changed; send RESTART first" }],
      },
    });
    fireEvent.click(startButton());
    expect(await screen.findByText("a calibration left Klipper changed; send RESTART first")).toBeTruthy();
  });

  it("shows_the_restart_instruction_only_when_needs_restart", async () => {
    await openWithFinished(finished());
    expect(await screen.findByText("The test finished.")).toBeTruthy();
    expect(screen.queryByText(RESTART_INSTRUCTION)).toBeNull();
  });

  it("shows the restart instruction when the run needs a restart", async () => {
    await openWithFinished(finished({ outcome: "failed", reason: "restore failed", needs_restart: true }));
    expect(await screen.findByText(RESTART_INSTRUCTION)).toBeTruthy();
  });

  it("links_to_the_session_when_done", async () => {
    await openWithFinished(finished());
    const link = await screen.findByRole("link", { name: "Open the session" });
    expect(link.getAttribute("href")).toBe("/sessions/run-1");
  });

  it("names the move of the running segment and draws no frequency chart", async () => {
    api();
    renderRoute("/run/axes-map");
    await waitFor(() => expect(FakeSocket.instances.length).toBeGreaterThan(0));
    act(() => {
      FakeSocket.latest.open();
      FakeSocket.latest.send({
        test: {
          id: "run-1",
          kind: "axes_map",
          phase: "measuring",
          segment: 2,
          segments: 3,
          axis: null,
          position: null,
          expected_segment_seconds: 4,
          elapsed_seconds: 1,
          frequency_hz: null,
          frequency_range: [0, 0],
          chips: [],
        },
        last: null,
      });
    });
    expect(await screen.findByText("2 of 3: Moving +Y")).toBeTruthy();
    expect(screen.queryByRole("img", { name: /Signal against frequency/ })).toBeNull();
  });
});

describe("a test of another kind on the axes-map run page", () => {
  beforeEach(() => stubWebSocket({ idle: true }));

  const running = (kind: TestKind): TestState => ({
    id: "test-9",
    kind,
    phase: "measuring",
    segment: 1,
    segments: 2,
    axis: null,
    position: [150, 150, 20],
    expected_segment_seconds: 100,
    elapsed_seconds: 5,
    frequency_hz: 20,
    frequency_range: [10, 135],
    chips: [],
  });

  async function openWith(snapshot: { test: TestState | null; last: FinishedTest | null }) {
    api();
    renderRoute("/run/axes-map");
    await waitFor(() => expect(FakeSocket.instances.length).toBeGreaterThan(0));
    act(() => {
      FakeSocket.latest.open();
      FakeSocket.latest.send(snapshot);
    });
  }

  it.each([
    ["axes", "A Input Shaper test is running.", "/run/shaper"],
    ["belts", "A Belts test is running.", "/run/belts"],
  ] as const)("a running %s test is a notice with a link and the form", async (other, text, to) => {
    await openWith({ test: running(other), last: null });
    expect(await screen.findByText(text)).toBeTruthy();
    expect(screen.getByRole("link", { name: /^Go to the/ }).getAttribute("href")).toBe(to);
    expect(await screen.findByRole("button", { name: /Start calibration|Checking printer/ })).toBeTruthy();
    expect(screen.queryByLabelText("Segment progress")).toBeNull();
  });

  it("a finished test of another kind shows the form, not the result", async () => {
    await openWith({ test: null, last: finished({ kind: "belts", outcome: "failed", reason: "boom" }) });
    expect(await screen.findByRole("button", { name: /Start calibration|Checking printer/ })).toBeTruthy();
    expect(screen.queryByText("boom")).toBeNull();
  });
});
