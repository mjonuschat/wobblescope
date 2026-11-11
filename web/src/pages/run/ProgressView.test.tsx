import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { FinishedTest } from "@/lib/generated/FinishedTest";
import type { TestState } from "@/lib/generated/TestState";
import { chipsSnapshot, printerSettings } from "@/test/capture";
import type { ApiCall, Reply } from "@/test/http";
import { renderRoute } from "@/test/render";
import { apiPath, stubApiWithStatus } from "@/test/status";
import { FakeSocket, stubWebSocket } from "@/test/websocket";

vi.mock("@/pages/session/SessionResults", () => ({
  SessionResults: ({ id }: { id: string }) => <div>session results for {id}</div>,
}));

vi.mock("@/components/charts/EChart", () => ({
  EChart: ({ label }: { label: string }) => <div role="img" aria-label={label} />,
}));

const running: TestState = {
  id: "test-1",
  phase: "measuring",
  segment: 3,
  segments: 4,
  axis: "X",
  position: [150.5, 154, 20],
  expected_segment_seconds: 145,
  elapsed_seconds: 12,
  frequency_hz: 17,
  chips: [
    { chip: "adxl345", level: [120, 40, 8], warnings: [] },
    { chip: "lis2dw ebb36", level: null, warnings: [] },
  ],
};

const done: FinishedTest = {
  id: "test-1",
  outcome: "complete",
  reason: null,
  session_id: "test-1",
  motion_uncertain: false,
};

/** The start button once the printer is known to be homed. */
async function enabledRun() {
  const button = (await screen.findByRole("button", { name: "Run Test" })) as HTMLButtonElement;
  await waitFor(() => expect(button.disabled).toBe(false));
  return button;
}

function api(start: Reply = { status: 202, body: { id: "test-1" } }) {
  return stubApiWithStatus((call: ApiCall) => {
    const path = apiPath(call);
    if (path === "settings") return { body: printerSettings() };
    if (path === "chips") return { body: chipsSnapshot() };
    if (path === "printer") return { body: { homed_axes: "xyz" } };
    if (path === "tests") return start;
    return { status: 404, body: { error: "not_found", message: "no" } };
  });
}

/** Renders the run screen and opens the live socket with `snapshot`. */
async function mount(snapshot: { test: TestState | null; last: FinishedTest | null }) {
  const calls = api();
  renderRoute("/run/shaper");
  await waitFor(() => expect(FakeSocket.instances.length).toBeGreaterThan(0));
  act(() => FakeSocket.latest.open());
  act(() => FakeSocket.latest.send(snapshot));
  return calls;
}

const send = (snapshot: { test: TestState | null; last: FinishedTest | null }) =>
  act(() => FakeSocket.latest.send(snapshot));

beforeEach(() => stubWebSocket());

const currentStep = () => document.querySelector('[aria-current="step"]')?.textContent;

describe("a running test", () => {
  it("shows the segment and the elapsed time and follows the latest event", async () => {
    await mount({ test: running, last: null });
    expect(await screen.findByText("Segment 3 of 4 (X at 150.5, 154, 20)")).toBeTruthy();
    expect(screen.getByText("12 s of about 2 min 25 s")).toBeTruthy();
    expect(screen.getByRole("img", { name: /Signal against frequency/ })).toBeTruthy();

    send({ test: { ...running, segment: 4, elapsed_seconds: 30 }, last: null });
    expect(screen.getByText(/Segment 4 of 4/)).toBeTruthy();
    expect(screen.getByText("30 s of about 2 min 25 s")).toBeTruthy();
  });

  it("opened mid-test goes straight to progress", async () => {
    await mount({ test: running, last: null });
    expect(await screen.findByText("Progress")).toBeTruthy();
    expect(currentStep()).toContain("Measuring");
    expect(screen.queryByRole("button", { name: "Run Test" })).toBeNull();
  });

  it("shows the shutdown notice when the test ended with an emergency stop", async () => {
    await mount({ test: running, last: null });
    send({ test: null, last: { ...done, outcome: "failed", reason: "emergency stop" } });
    fireEvent.click(await screen.findByRole("button", { name: "Next" }));
    expect(await screen.findByText("The test failed.")).toBeTruthy();
    expect(screen.getAllByText(/FIRMWARE_RESTART/).length).toBeGreaterThan(0);
  });

  describe("when the test ends", () => {
    const nextButton = () => screen.getByRole("button", { name: "Next" }) as HTMLButtonElement;

    it("keeps Next disabled while the test runs", async () => {
      await mount({ test: running, last: null });
      await screen.findByText("Progress");
      expect(nextButton().disabled).toBe(true);
    });

    it("stays on the measuring step with the last progress, and Next moves on at once", async () => {
      await mount({ test: running, last: null });
      await screen.findByText("Progress");
      send({ test: null, last: done });
      expect(await screen.findByText("The test finished.")).toBeTruthy();
      expect(currentStep()).toContain("Measuring");
      expect(screen.getByText(/Segment 3 of 4/)).toBeTruthy();
      expect(screen.getByText("Continuing to the results in 5 s")).toBeTruthy();
      expect(nextButton().disabled).toBe(false);
      fireEvent.click(nextButton());
      await waitFor(() => expect(currentStep()).toContain("Results"));
      expect(screen.getByRole("button", { name: "New test" })).toBeTruthy();
    });

    it("goes on to the results by itself after five seconds", async () => {
      await mount({ test: running, last: null });
      await screen.findByText("Progress");
      vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
      try {
        send({ test: null, last: done });
        expect(screen.getByText("Continuing to the results in 5 s")).toBeTruthy();
        await act(async () => {
          vi.advanceTimersByTime(2000);
        });
        expect(screen.getByText("Continuing to the results in 3 s")).toBeTruthy();
        expect(currentStep()).toContain("Measuring");
        await act(async () => {
          vi.advanceTimersByTime(3000);
        });
        expect(currentStep()).toContain("Results");
      } finally {
        vi.useRealTimers();
      }
    });
  });

  it("has no stop button of its own; the header has the printer's", async () => {
    await mount({ test: running, last: null });
    await screen.findByText("Progress");
    expect(screen.queryByRole("button", { name: "Emergency stop" })).toBeNull();
    expect(screen.getByRole("button", { name: "Stop printer" })).toBeTruthy();
  });

  it("says the failure is recorded while the sweep ends", async () => {
    await mount({ test: { ...running, phase: "waiting_for_sweep" }, last: null });
    expect(await screen.findByText("Failure recorded, waiting for the sweep to end")).toBeTruthy();
  });

  describe("chip warnings", () => {
    const warned = (segment = 3): TestState => ({
      ...running,
      segment,
      chips: [
        { chip: "adxl345", level: [1, 1, 1], warnings: ["the chip's errors counter rose by 3"] },
        running.chips[1],
      ],
    });
    const warning = () => screen.queryAllByText("the chip's errors counter rose by 3");

    it("shows a warning once as a toast with its chip's name", async () => {
      await mount({ test: running, last: null });
      send({ test: warned(), last: null });
      expect(await screen.findByText("the chip's errors counter rose by 3")).toBeTruthy();
      expect(screen.getByText("adxl345")).toBeTruthy();
      send({ test: warned(4), last: null });
      send({ test: warned(4), last: null });
      expect(warning()).toHaveLength(1);
    });

    it("stays when the test ends and goes when it is closed", async () => {
      await mount({ test: running, last: null });
      send({ test: warned(), last: null });
      await screen.findByText("the chip's errors counter rose by 3");
      send({ test: null, last: done });
      expect(await screen.findByText("The test finished.")).toBeTruthy();
      expect(warning()).toHaveLength(1);
      fireEvent.click(screen.getByRole("button", { name: /close/i }));
      await waitFor(() => expect(warning()).toHaveLength(0), { timeout: 3000 });
    });

    it("goes when the user moves to another page", async () => {
      await mount({ test: running, last: null });
      send({ test: warned(), last: null });
      await screen.findByText("the chip's errors counter rose by 3");
      fireEvent.click(screen.getByRole("link", { name: "History" }));
      await waitFor(() => expect(warning()).toHaveLength(0), { timeout: 3000 });
    });
  });

  it("keeps the last state when the socket drops, and a new snapshot clears the mark", async () => {
    await mount({ test: running, last: null });
    await screen.findByText("Progress");
    act(() => FakeSocket.latest.drop());
    expect(await screen.findByText(/Connection lost, this may be out of date/)).toBeTruthy();
    expect(screen.getByText(/Segment 3 of 4/)).toBeTruthy();

    // The page reconnects on its own; the new snapshot replaces the old state.
    await waitFor(() => expect(FakeSocket.instances.length).toBeGreaterThan(1), { timeout: 3000 });
    act(() => FakeSocket.latest.open());
    send({ test: { ...running, segment: 4 }, last: null });
    expect(screen.queryByText(/Connection lost/)).toBeNull();
    expect(screen.getByText(/Segment 4 of 4/)).toBeTruthy();
  });
});

describe("a finished test", () => {
  it("shows the session's results under a complete or incomplete test", async () => {
    await mount({ test: null, last: done });
    expect(await screen.findByText("session results for test-1")).toBeTruthy();
  });

  it("shows no session results under a failed test", async () => {
    await mount({ test: null, last: { ...done, outcome: "failed", reason: "x" } });
    await screen.findByText("The test failed.");
    expect(screen.queryByText(/session results for/)).toBeNull();
  });

  it("shows only the reason when no session exists", async () => {
    await mount({
      test: null,
      last: { ...done, outcome: "no_session", session_id: null, reason: "Klipper is shutdown, not ready" },
    });
    expect(await screen.findByText("Klipper is shutdown, not ready")).toBeTruthy();
    expect(screen.getByText("No session was saved.")).toBeTruthy();
  });

  it("keeps a warning while motion is uncertain, until an event clears it", async () => {
    await mount({
      test: null,
      last: { ...done, outcome: "failed", reason: "the sweep's outcome is unknown", motion_uncertain: true },
    });
    expect(await screen.findByText("The printer may still be moving.")).toBeTruthy();
    expect(screen.getByText(/stop button at the top right/)).toBeTruthy();

    send({ test: null, last: { ...done, outcome: "failed", reason: "x", motion_uncertain: false } });
    expect(screen.queryByText("The printer may still be moving.")).toBeNull();
  });

  it("does not let New test hide the warning while motion is uncertain", async () => {
    await mount({
      test: null,
      last: { ...done, outcome: "failed", reason: "the sweep's outcome is unknown", motion_uncertain: true },
    });
    const newTest = await screen.findByRole("button", { name: "New test" });
    expect((newTest as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(newTest);
    expect(screen.getByText("The printer may still be moving.")).toBeTruthy();

    send({ test: null, last: { ...done, outcome: "failed", reason: "x", motion_uncertain: false } });
    expect((screen.getByRole("button", { name: "New test" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("stays on screen until New test is clicked, and a reload after the end shows it too", async () => {
    await mount({ test: null, last: done });
    expect(await screen.findByText("The test finished.")).toBeTruthy();
    expect(currentStep()).toContain("Results");
    // Nothing else on the page takes it away.
    send({ test: null, last: done });
    expect(screen.getByText("The test finished.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "New test" }));
    expect(await screen.findByRole("button", { name: "Run Test" })).toBeTruthy();
    expect(screen.queryByText("The test finished.")).toBeNull();
    expect(currentStep()).toContain("Setup");
  });

  it("goes back to setup when the Setup step is clicked on the results", async () => {
    await mount({ test: null, last: done });
    fireEvent.click(await screen.findByRole("button", { name: /Setup/ }));
    expect(await screen.findByRole("button", { name: "Run Test" })).toBeTruthy();
  });

  it("does not let the Setup step be clicked while the printer may still be moving", async () => {
    await mount({ test: null, last: { ...done, outcome: "failed", reason: "x", motion_uncertain: true } });
    await screen.findByText("The printer may still be moving.");
    expect(screen.queryByRole("button", { name: /Setup/ })).toBeNull();
  });
});

describe("starting a test", () => {
  it("says the live view is not connected when the socket never delivers a snapshot", async () => {
    api();
    renderRoute("/run/shaper");
    // The socket is never opened, as when a proxy refuses the upgrade.
    fireEvent.click(await enabledRun());
    expect(await screen.findByText("Starting…")).toBeTruthy();
    expect(screen.getByText(/The live view is not connected/)).toBeTruthy();
  });

  it("shows Starting… after the 202 and progress once the live state names the test", async () => {
    api();
    renderRoute("/run/shaper");
    await waitFor(() => expect(FakeSocket.instances.length).toBeGreaterThan(0));
    act(() => FakeSocket.latest.open());
    send({ test: null, last: null });
    fireEvent.click(await enabledRun());
    expect(await screen.findByText("Starting…")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Run Test" })).toBeNull();

    send({ test: running, last: null });
    expect(await screen.findByText("Progress")).toBeTruthy();
    expect(screen.queryByText("Starting…")).toBeNull();
  });
});
