import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { getMotorRegisters } from "@/lib/api/endpoints";
import type { MotorRegisters } from "@/lib/generated/MotorRegisters";
import type { MotorTuning } from "@/lib/generated/MotorTuning";

import { MotorTuningCard } from "./MotorTuningCard";

vi.mock("@/lib/api/endpoints", async (original) => ({
  ...(await original<typeof import("@/lib/api/endpoints")>()),
  getMotorRegisters: vi.fn(),
}));

const read = vi.mocked(getMotorRegisters);

function renderCard(tuning: MotorTuning[]) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MotorTuningCard tuning={tuning} />
    </QueryClientProvider>,
  );
}

function registers(stepper: string, over: Partial<MotorRegisters> = {}): MotorRegisters {
  return {
    stepper,
    registers: [
      { name: "CHOPCONF", value: "0x330081e1" },
      { name: "PWMCONF", value: "0x4f0e0516" },
    ],
    differs_from: null,
    differs: [],
    ...over,
  };
}

beforeEach(() => {
  read.mockReset();
  read.mockImplementation((stepper) => Promise.resolve(registers(stepper)));
});

function entry(stepper: string, over: Partial<MotorTuning> = {}): MotorTuning {
  return {
    stepper,
    driver: "tmc2240",
    run_current: 1,
    hold_current: 1,
    microsteps: 32,
    motor: "ldo-42sth40-1684cl350et",
    tuning_goal: "auto",
    extra_hysteresis: 0,
    tbl: 1,
    toff: 0,
    tpfd: null,
    sgt: 1,
    sg4_thrs: 0,
    semin: 2,
    semax: 4,
    seup: 3,
    sedn: 2,
    seimin: 1,
    voltage: 24,
    overvoltage_vth: null,
    pwm_freq_target: 20000,
    differs_from: null,
    differs: [],
    ...over,
  };
}

describe("the motor tuning card", () => {
  it("renders nothing when there are no autotune sections", () => {
    const { container } = renderCard([]);
    expect(container.textContent).toBe("");
  });

  it("shows one row per stepper with its currents to two decimals", () => {
    renderCard([
      entry("extruder", { driver: null, run_current: null, hold_current: null, microsteps: null }),
      entry("stepper_z", { run_current: 0.8, hold_current: 0.55 }),
    ]);
    expect(screen.getByText("Motor tuning")).toBeTruthy();
    const rows = screen.getAllByRole("row");
    expect(rows).toHaveLength(3);
    expect(within(rows[1]).getAllByText("–")).toHaveLength(4);
    expect(within(rows[2]).getByText("stepper_z")).toBeTruthy();
    expect(within(rows[2]).getByText("0.80")).toBeTruthy();
    expect(within(rows[2]).getByText("0.55")).toBeTruthy();
    expect(within(rows[2]).getByText("32")).toBeTruthy();
  });

  it("opens the autotune values, voltage first, and hides the ones that are not set", () => {
    renderCard([entry("stepper_z")]);
    expect(screen.queryByText("pwm_freq_target")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show all values for stepper_z" }));
    expect(screen.getByRole("heading", { name: "Autotune" })).toBeTruthy();
    const labels = screen.getAllByRole("term").map((term) => term.textContent);
    expect(labels[0]).toBe("voltage");
    expect(labels).toContain("pwm_freq_target");
    expect(labels).not.toContain("tpfd");
    expect(screen.getByText("20000")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Hide all values for stepper_z" }));
    expect(screen.queryByText("pwm_freq_target")).toBeNull();
  });

  it("has nothing to open on a stepper with neither a driver nor autotune values", () => {
    const none = entry("stepper_x", {
      driver: null,
      motor: null,
      tuning_goal: null,
      extra_hysteresis: null,
      tbl: null,
      toff: null,
      sgt: null,
      sg4_thrs: null,
      semin: null,
      semax: null,
      seup: null,
      sedn: null,
      seimin: null,
      voltage: null,
      pwm_freq_target: null,
    });
    renderCard([none, entry("stepper_z", { semin: null })]);
    const rows = screen.getAllByRole("row");
    expect(within(rows[1]).queryByRole("button")).toBeNull();
    expect(within(rows[2]).getByRole("button")).toBeTruthy();
  });

  it("warns that two motors are configured differently and marks what differs on the second", () => {
    const x = entry("stepper_x", { run_current: 1.2, hold_current: 1.2 });
    const y = entry("stepper_y", {
      run_current: 1,
      hold_current: 1,
      tuning_goal: "silent",
      differs_from: "stepper_x",
      differs: ["tuning_goal", "run_current", "hold_current"],
    });
    renderCard([x, y]);
    expect(screen.getByText("stepper_x and stepper_y have different configurations")).toBeTruthy();
    const second = screen.getAllByRole("row")[2];
    expect(within(second).getByTitle("stepper_x: auto")).toBeTruthy();
    const [run, hold] = within(second).getAllByTitle("stepper_x: 1.20");
    expect(run.className).toContain("bg-warning");
    expect(hold.className).toContain("bg-warning");
    expect(within(second).getByText("ldo-42sth40-1684cl350et").className).not.toContain("bg-warning");
    expect(within(screen.getAllByRole("row")[1]).queryByTitle(/stepper_/)).toBeNull();
  });

  it("marks a closed row whose autotune values differ, and the value once it is open", () => {
    const x = entry("stepper_x", { tbl: 1 });
    const y = entry("stepper_y", { tbl: 2, differs_from: "stepper_x", differs: ["tbl"] });
    renderCard([x, y]);
    expect(screen.getByRole("img", { name: "Some values differ from the other motor" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Show all values for stepper_y" }));
    expect(screen.queryByRole("img", { name: "Some values differ from the other motor" })).toBeNull();
    expect(screen.getByTitle("stepper_x: 1")).toBeTruthy();
  });

  it("shows no warning when nothing differs", () => {
    renderCard([entry("stepper_x"), entry("stepper_y")]);
    expect(screen.queryByText(/have different configurations/)).toBeNull();
  });

  it("reads no registers until a row is opened, then reads them again each time it opens", async () => {
    renderCard([entry("stepper_z")]);
    expect(read).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Show all values for stepper_z" }));
    expect(await screen.findByText("0x330081e1")).toBeTruthy();
    expect(read).toHaveBeenCalledTimes(1);
    expect(read.mock.calls[0][0]).toBe("stepper_z");
    fireEvent.click(screen.getByRole("button", { name: "Hide all values for stepper_z" }));
    fireEvent.click(screen.getByRole("button", { name: "Show all values for stepper_z" }));
    await waitFor(() => expect(read).toHaveBeenCalledTimes(2));
  });

  it("shows the registers under their own heading, after the autotune values", async () => {
    renderCard([entry("stepper_z")]);
    fireEvent.click(screen.getByRole("button", { name: "Show all values for stepper_z" }));
    expect(screen.getByText("Reading registers…")).toBeTruthy();
    expect(await screen.findByText("PWMCONF")).toBeTruthy();
    const headings = screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent);
    expect(headings).toEqual(["Autotune", "Registers"]);
    expect(screen.getByText("0x4f0e0516")).toBeTruthy();
  });

  it("opens a stepper that has a driver and no autotune values and shows only its registers", async () => {
    const bare = entry("stepper_x", {
      motor: null,
      tuning_goal: null,
      extra_hysteresis: null,
      tbl: null,
      toff: null,
      sgt: null,
      sg4_thrs: null,
      semin: null,
      semax: null,
      seup: null,
      sedn: null,
      seimin: null,
      voltage: null,
      pwm_freq_target: null,
    });
    renderCard([bare]);
    fireEvent.click(screen.getByRole("button", { name: "Show all values for stepper_x" }));
    expect(await screen.findByText("0x330081e1")).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Autotune" })).toBeNull();
    expect(screen.getByRole("heading", { name: "Registers" })).toBeTruthy();
  });

  it("reads nothing for a stepper without a driver", () => {
    renderCard([entry("stepper_q", { driver: null })]);
    fireEvent.click(screen.getByRole("button", { name: "Show all values for stepper_q" }));
    expect(read).not.toHaveBeenCalled();
    expect(screen.queryByRole("heading", { name: "Registers" })).toBeNull();
  });

  it("says why the registers could not be read", async () => {
    read.mockRejectedValue(new Error("a print is printing; wait for it to end or cancel it"));
    renderCard([entry("stepper_z")]);
    fireEvent.click(screen.getByRole("button", { name: "Show all values for stepper_z" }));
    expect(await screen.findByText("a print is printing; wait for it to end or cancel it")).toBeTruthy();
  });

  it("marks the registers of the second motor that differ from the first", async () => {
    read.mockResolvedValue(
      registers("stepper_y", {
        differs_from: "stepper_x",
        differs: [{ name: "PWMCONF", other: "0x4f0e0517" }],
      }),
    );
    renderCard([entry("stepper_x"), entry("stepper_y")]);
    fireEvent.click(screen.getByRole("button", { name: "Show all values for stepper_y" }));
    const value = await screen.findByTitle("stepper_x: 0x4f0e0517");
    expect(value.parentElement?.parentElement?.className).toContain("bg-warning");
    expect(screen.getByText("0x330081e1").closest("div")?.className).not.toContain("bg-warning");
  });
});
