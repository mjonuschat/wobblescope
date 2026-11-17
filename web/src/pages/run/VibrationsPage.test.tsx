import { act, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { PrinterSettings } from "@/lib/generated/PrinterSettings";
import { chipsSnapshot, printerSettings } from "@/test/capture";
import type { ApiCall } from "@/test/http";
import { renderRoute } from "@/test/render";
import { apiPath, stubApiWithStatus } from "@/test/status";
import { FakeSocket, stubWebSocket } from "@/test/websocket";

vi.mock("@/components/charts/EChart", () => ({
  EChart: ({ label }: { label: string }) => <div role="img" aria-label={label} />,
}));

function withKinematics(kinematics: string | null): PrinterSettings {
  const base = printerSettings();
  return { ...base, machine: { ...base.machine, kinematics } };
}

function api(settings: PrinterSettings = printerSettings(), busy: string | null = null) {
  return stubApiWithStatus((call: ApiCall) => {
    const path = apiPath(call);
    if (path === "settings") return { body: settings };
    if (path === "chips") return { body: chipsSnapshot() };
    if (path === "printer") return { body: { homed_axes: "xyz", busy } };
    if (path === "vibrations" && call.method === "POST") return { status: 202, body: { id: "run-1" } };
    return { status: 404, body: { error: "not_found", message: "no" } };
  });
}

const posted = (calls: ApiCall[]) => calls.filter((c) => c.method === "POST" && apiPath(c) === "vibrations");
const field = (label: RegExp | string) => screen.getByLabelText(label) as HTMLInputElement;
const startButton = () => screen.getByRole("button", { name: /Run Test|Checking printer/ }) as HTMLButtonElement;

async function open(settings?: PrinterSettings) {
  const calls = api(settings);
  renderRoute("/run/vibrations");
  await screen.findByRole("button", { name: "Run Test" });
  await waitFor(() => expect(startButton().disabled).toBe(false));
  return calls;
}

describe("the vibrations run page", () => {
  beforeEach(() => stubWebSocket({ idle: true }));

  it("is_titled_and_shows_the_parameters_with_their_defaults", async () => {
    await open();
    expect(screen.getByRole("heading", { name: "Vibrations profile" })).toBeTruthy();
    expect(field(/^Size/).value).toBe("100");
    expect(field(/^Z Height/).value).toBe("20");
    expect(field(/^Max Speed/).value).toBe("200");
    expect(field(/^Speed Increment/).value).toBe("2");
    expect(field(/^Acceleration/).value).toBe("3000");
    expect(field(/^Travel Speed/).value).toBe("120");
  });

  it("refuses_a_value_under_its_minimum", async () => {
    const calls = await open();
    const cases: [RegExp, string, string][] = [
      [/^Size/, "49", "Size must be at least 50 mm."],
      [/^Max Speed/, "9", "Max Speed must be at least 10 mm/s."],
      [/^Speed Increment/, "0.5", "Speed Increment must be at least 1 mm/s."],
      [/^Acceleration/, "99", "Acceleration must be at least 100 mm/s\u00b2."],
      [/^Travel Speed/, "19", "Travel Speed must be at least 20 mm/s."],
    ];
    for (const [label, value, message] of cases) {
      fireEvent.change(field(label), { target: { value } });
      expect(screen.getByText(message)).toBeTruthy();
      expect(startButton().disabled).toBe(true);
      fireEvent.change(field(label), { target: { value: "" } });
    }
    fireEvent.change(field(/^Size/), { target: { value: "49" } });
    fireEvent.submit(startButton().closest("form")!);
    expect(posted(calls)).toHaveLength(0);
    fireEvent.change(field(/^Size/), { target: { value: "50" } });
    expect(startButton().disabled).toBe(false);
  });

  it("posts_the_chosen_chips_and_parameters", async () => {
    const calls = await open();
    fireEvent.click(screen.getByRole("switch", { name: "adxl345" }));
    fireEvent.change(field(/^Size/), { target: { value: "120" } });
    fireEvent.change(field(/^Z Height/), { target: { value: "40" } });
    fireEvent.change(field(/^Acceleration/), { target: { value: "2500" } });
    fireEvent.click(startButton());
    await waitFor(() => expect(posted(calls)).toHaveLength(1));
    expect(JSON.parse(String(posted(calls)[0].body))).toEqual({
      chips: ["adxl345", "lis2dw ebb36"],
      size: 120,
      z_height: 40,
      accel: 2500,
    });
  });

  it("sends_only_the_chips_when_no_field_was_changed", async () => {
    const calls = await open();
    fireEvent.click(startButton());
    await waitFor(() => expect(posted(calls)).toHaveLength(1));
    expect(JSON.parse(String(posted(calls)[0].body))).toEqual({ chips: ["lis2dw ebb36"] });
  });

  it("refuses_to_start_with_no_chip_chosen", async () => {
    await open();
    fireEvent.click(screen.getByRole("switch", { name: "lis2dw ebb36" }));
    expect(screen.getByText("Choose at least one chip.")).toBeTruthy();
    expect(startButton().disabled).toBe(true);
  });

  it("keeps_start_disabled_while_the_printer_is_busy", async () => {
    api(printerSettings(), "The printer is homing.");
    renderRoute("/run/vibrations");
    expect(await screen.findByText("The printer is homing.")).toBeTruthy();
    expect(startButton().disabled).toBe(true);
  });

  it("is_unavailable_on_unsupported_kinematics", async () => {
    for (const kinematics of ["delta", null]) {
      api(withKinematics(kinematics));
      renderRoute("/run/vibrations");
      expect(await screen.findByText("A vibrations profile needs a Cartesian, CoreXY or CoreXZ printer.")).toBeTruthy();
      expect(screen.queryByRole("button", { name: /Run Test/ })).toBeNull();
      cleanup();
    }
  });

  it("shows_the_signal_levels_and_no_frequency_chart_for_a_running_vibrations_test", async () => {
    api();
    renderRoute("/run/vibrations");
    await waitFor(() => expect(FakeSocket.instances.length).toBeGreaterThan(0));
    act(() => {
      FakeSocket.latest.open();
      FakeSocket.latest.send({
        test: {
          id: "run-1",
          kind: "vibrations",
          phase: "measuring",
          segment: 1,
          segments: 2,
          axis: null,
          position: null,
          expected_segment_seconds: 4,
          elapsed_seconds: 1,
          frequency_hz: null,
          frequency_range: [0, 0],
          chips: [{ chip: "adxl345", level: [120, 40, 8], warnings: [] }],
        },
        last: null,
      });
    });
    expect(await screen.findByRole("region", { name: "Signal level of adxl345" })).toBeTruthy();
    expect(screen.queryByRole("img", { name: /Signal against frequency/ })).toBeNull();
  });

  it("stops_waiting_for_a_run_the_daemon_no_longer_knows", async () => {
    await open();
    fireEvent.click(startButton());
    expect(await screen.findByText("Starting…")).toBeTruthy();
    // The daemon restarted, so its next snapshot has neither the run nor a finished one.
    act(() => FakeSocket.latest.send({ test: null, last: null }));
    expect(await screen.findByRole("button", { name: "Run Test" })).toBeTruthy();
    expect(screen.queryByText("Starting…")).toBeNull();
  });
});
