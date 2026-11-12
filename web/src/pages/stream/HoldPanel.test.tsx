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

function renderPanel(api: HoldApi, frequencyOfInterest: number | null = null, streaming = true) {
  const calls = stubApiWithStatus((call) =>
    apiPath(call) === "settings" ? { body: printerSettings() } : { body: { dispatched: true } },
  );
  render(
    <QueryClientProvider client={createQueryClient()}>
      <TooltipProvider>
        <HoldPanel hold={api} frequencyOfInterest={frequencyOfInterest} streaming={streaming} />
      </TooltipProvider>
    </QueryClientProvider>,
  );
  return calls;
}

describe("HoldPanel", () => {
  it("defaults to 25 Hz and the printer's acceleration per Hz", async () => {
    renderPanel(hold());
    expect(await screen.findByDisplayValue("60")).toBeTruthy();
    expect((screen.getByLabelText("Frequency") as HTMLInputElement).value).toBe("25");
    expect(screen.getByText("Hz")).toBeTruthy();
    expect(screen.getByText("mm/s² per Hz")).toBeTruthy();
    expect(screen.getByText("Default")).toBeTruthy();
  });

  it("defaults the frequency to the frequency of interest", async () => {
    renderPanel(hold(), 52);
    await screen.findByDisplayValue("60");
    expect((screen.getByLabelText("Frequency") as HTMLInputElement).value).toBe("52");
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
      duration_seconds: null,
      start_point: null,
    });
  });

  it("will not start while no chip is streaming", async () => {
    const api = hold();
    renderPanel(api, null, false);
    await screen.findByDisplayValue("60");
    const start = screen.getByRole("button", { name: "Start" }) as HTMLButtonElement;
    expect(start.disabled).toBe(true);
  });

  it("will not start on nonsense", async () => {
    renderPanel(hold());
    await screen.findByDisplayValue("60");
    const start = screen.getByRole("button", { name: "Start" }) as HTMLButtonElement;
    const frequency = screen.getByLabelText("Frequency");
    for (const bad of ["4", "201", ""]) {
      fireEvent.change(frequency, { target: { value: bad } });
      expect(start.disabled, `frequency ${bad}`).toBe(true);
    }
    fireEvent.change(frequency, { target: { value: "60" } });
    expect(start.disabled).toBe(false);
    fireEvent.change(screen.getByLabelText("Acceleration per Hz"), { target: { value: "0" } });
    expect(start.disabled).toBe(true);
  });

  it("says what the toolhead is doing only while it travels or stops", async () => {
    const { rerender } = render(
      <QueryClientProvider client={createQueryClient()}>
        <TooltipProvider>
          <HoldPanel
            hold={hold({ status: "running", state: { ...RUNNING, phase: "travelling" } })}
            frequencyOfInterest={null}
            streaming
          />
        </TooltipProvider>
      </QueryClientProvider>,
    );
    expect(await screen.findByText(/moving to the start point/i)).toBeTruthy();
    rerender(
      <QueryClientProvider client={createQueryClient()}>
        <TooltipProvider>
          <HoldPanel hold={hold({ status: "running", state: RUNNING })} frequencyOfInterest={null} streaming />
        </TooltipProvider>
      </QueryClientProvider>,
    );
    expect(screen.queryByText(/moving to the start point/i)).toBeNull();
    expect(screen.queryByText(/elapsed/i)).toBeNull();
  });

  it("sends set when the frequency changes during a hold", async () => {
    const api = hold({ status: "running", id: "hold-1", state: RUNNING });
    renderPanel(api);
    await screen.findByDisplayValue("60");
    fireEvent.change(screen.getByLabelText("Frequency"), { target: { value: "70" } });
    expect(api.set).toHaveBeenCalledWith({ frequency: 70 });
    fireEvent.change(screen.getByLabelText("Frequency"), { target: { value: "300" } });
    expect(api.set).toHaveBeenCalledTimes(1);
  });

  it("sends stop", async () => {
    const api = hold({ status: "running", id: "hold-1", state: RUNNING });
    renderPanel(api);
    fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
    expect(api.stop).toHaveBeenCalled();
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

  it("says why a hold failed and nothing about one that ended normally", async () => {
    renderPanel(
      hold({ status: "ended", ended: { reason: { failed: { message: "Move out of range" } }, restore: "done" } }),
    );
    expect(await screen.findByText(/Move out of range/)).toBeTruthy();
    document.body.replaceChildren();
    for (const reason of ["stopped", "time_limit", "connection_lost", "emergency_stop"] as const) {
      const { unmount } = render(
        <QueryClientProvider client={createQueryClient()}>
          <TooltipProvider>
            <HoldPanel
              hold={hold({ status: "ended", ended: { reason, restore: "done" } })}
              frequencyOfInterest={null}
              streaming
            />
          </TooltipProvider>
        </QueryClientProvider>,
      );
      expect(screen.queryByText(/stopped|time limit|connection|emergency/i), reason).toBeNull();
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
