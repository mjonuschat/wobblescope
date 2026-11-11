import { QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import { createQueryClient } from "@/lib/api/queries";
import { printerSettings } from "@/test/capture";
import { apiPath, stubApiWithStatus } from "@/test/status";

import { HoldPanel } from "./HoldPanel";
import type { HoldApi } from "./useHold";

function hold(over: Partial<HoldApi> = {}): HoldApi {
  return {
    status: "idle",
    id: null,
    state: null,
    rejected: null,
    failures: [],
    ended: null,
    error: null,
    start: vi.fn(),
    set: vi.fn(),
    stop: vi.fn(),
    ...over,
  };
}

const RUNNING = {
  phase: "holding" as const,
  elapsed_seconds: 12.4,
  frequency: 60,
  axis: "X" as const,
  acceleration: 3600,
  accel_limit: 3600,
};

function renderPanel(api: HoldApi, frequencyOfInterest: number | null = null) {
  const calls = stubApiWithStatus((call) =>
    apiPath(call) === "settings" ? { body: printerSettings() } : { body: { dispatched: true } },
  );
  render(
    <QueryClientProvider client={createQueryClient()}>
      <TooltipProvider>
        <HoldPanel hold={api} frequencyOfInterest={frequencyOfInterest} />
      </TooltipProvider>
    </QueryClientProvider>,
  );
  return calls;
}

describe("HoldPanel", () => {
  it("defaults to 25 Hz and the printer's acceleration per Hz", async () => {
    renderPanel(hold());
    expect(await screen.findByDisplayValue("60")).toBeTruthy();
    expect((screen.getByLabelText("Frequency (Hz)") as HTMLInputElement).value).toBe("25");
    expect(screen.getByText("30 s")).toBeTruthy();
    expect(screen.getByText("Test point")).toBeTruthy();
  });

  it("defaults the frequency to the frequency of interest", async () => {
    renderPanel(hold(), 52);
    await screen.findByDisplayValue("60");
    expect((screen.getByLabelText("Frequency (Hz)") as HTMLInputElement).value).toBe("52");
  });

  it("starts with what the form holds", async () => {
    const api = hold();
    renderPanel(api, 52);
    await screen.findByDisplayValue("60");
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    expect(api.start).toHaveBeenCalledWith({
      axis: "X",
      frequency: 52,
      accel_per_hz: 60,
      duration_seconds: 30,
      start_point: null,
    });
  });

  it("will not start on nonsense", async () => {
    renderPanel(hold());
    await screen.findByDisplayValue("60");
    const start = screen.getByRole("button", { name: "Start" }) as HTMLButtonElement;
    const frequency = screen.getByLabelText("Frequency (Hz)");
    for (const bad of ["4", "201", ""]) {
      fireEvent.change(frequency, { target: { value: bad } });
      expect(start.disabled, `frequency ${bad}`).toBe(true);
    }
    fireEvent.change(frequency, { target: { value: "60" } });
    expect(start.disabled).toBe(false);
    fireEvent.change(screen.getByLabelText("Acceleration per Hz"), { target: { value: "0" } });
    expect(start.disabled).toBe(true);
  });

  it("shows the numbers and the limit while it runs", async () => {
    renderPanel(hold({ status: "running", id: "hold-1", state: RUNNING }));
    expect(await screen.findByText(/12\.4 s/)).toBeTruthy();
    expect(screen.getByText(/3600 mm\/s²/)).toBeTruthy();
    expect(screen.getByText(/limit/i)).toBeTruthy();
  });

  it("sends set when the frequency changes during a hold", async () => {
    const api = hold({ status: "running", id: "hold-1", state: RUNNING });
    renderPanel(api);
    await screen.findByDisplayValue("60");
    fireEvent.change(screen.getByLabelText("Frequency (Hz)"), { target: { value: "70" } });
    expect(api.set).toHaveBeenCalledWith({ frequency: 70 });
    fireEvent.change(screen.getByLabelText("Frequency (Hz)"), { target: { value: "300" } });
    expect(api.set).toHaveBeenCalledTimes(1);
  });

  it("sends stop", async () => {
    const api = hold({ status: "running", id: "hold-1", state: RUNNING });
    renderPanel(api);
    fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
    expect(api.stop).toHaveBeenCalled();
  });

  it("posts an emergency stop to the hold's id after two clicks", async () => {
    const api = hold({ status: "running", id: "hold-2026-11-11T10:00:00Z", state: RUNNING });
    const calls = renderPanel(api);
    const button = await screen.findByRole("button", { name: /emergency stop/i });
    fireEvent.click(button);
    expect(calls.find((c) => apiPath(c).includes("emergency-stop"))).toBeUndefined();
    fireEvent.click(screen.getByRole("button", { name: /press again/i }));
    await vi.waitFor(() => {
      const call = calls.find((c) => apiPath(c).includes("emergency-stop"));
      expect(call && apiPath(call)).toBe("tests/hold-2026-11-11T10%3A00%3A00Z/emergency-stop");
    });
  });

  it("disables the emergency stop until the id has arrived", async () => {
    renderPanel(hold({ status: "starting" }));
    const button = (await screen.findByRole("button", { name: /emergency stop/i })) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
  });

  it("lists every failure and points at the Home button", async () => {
    renderPanel(
      hold({
        failures: [
          { check: "homed", message: "home X, Y and Z first; not homed: Z" },
          { check: "not_printing", message: "a print is paused" },
        ],
      }),
    );
    expect(await screen.findByText(/a print is paused/)).toBeTruthy();
    const homed = screen.getByText(/not homed: Z/);
    expect(homed.textContent).toMatch(/Home button in the page header/);
  });

  it("says in words why a hold ended", async () => {
    const cases: [HoldApi["ended"], RegExp][] = [
      [{ reason: "time_limit", restore: "done" }, /time limit was reached/i],
      [{ reason: "stopped", restore: "done" }, /stopped/i],
      [{ reason: "connection_lost", restore: "done" }, /connection to this page was lost/i],
      [{ reason: { failed: { message: "Move out of range" } }, restore: "done" }, /Move out of range/],
    ];
    for (const [ended, text] of cases) {
      const { unmount } = (() => {
        renderPanel(hold({ status: "ended", ended }));
        return { unmount: () => document.body.replaceChildren() };
      })();
      expect(await screen.findByText(text)).toBeTruthy();
      unmount();
    }
  });

  it("shows what is left and the fix when the restore failed", async () => {
    renderPanel(
      hold({
        status: "ended",
        ended: {
          reason: "stopped",
          restore: {
            failed: {
              left_changed: ["max_accel 3000 (the hold raised it)", "input shaping (the hold turned it off)"],
              fix: "SET_VELOCITY_LIMIT VELOCITY=500 ACCEL=3000\nG90",
            },
          },
        },
      }),
    );
    expect(await screen.findByText(/max_accel 3000/)).toBeTruthy();
    expect(screen.getByText(/input shaping/)).toBeTruthy();
    expect(screen.getByText(/SET_VELOCITY_LIMIT VELOCITY=500/)).toBeTruthy();
  });

  it("shows a lost connection and a rejected change", async () => {
    renderPanel(hold({ status: "ended", error: "The connection to the service was lost." }));
    expect(await screen.findByText(/connection to the service was lost/i)).toBeTruthy();
    document.body.replaceChildren();
    renderPanel(hold({ status: "running", id: "hold-1", state: RUNNING, rejected: "axis Y has no room" }));
    expect(await screen.findByText(/axis Y has no room/)).toBeTruthy();
  });
});
