import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { MotorTuning } from "@/lib/generated/MotorTuning";

import { MotorTuningCard } from "./MotorTuningCard";

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
    const { container } = render(<MotorTuningCard tuning={[]} />);
    expect(container.textContent).toBe("");
  });

  it("shows one row per stepper with its currents to two decimals", () => {
    render(
      <MotorTuningCard
        tuning={[
          entry("extruder", { driver: null, run_current: null, hold_current: null, microsteps: null }),
          entry("stepper_z", { run_current: 0.8, hold_current: 0.55 }),
        ]}
      />,
    );
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
    render(<MotorTuningCard tuning={[entry("stepper_z")]} />);
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

  it("has nothing to open on a stepper without autotune values", () => {
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
    render(<MotorTuningCard tuning={[bare, entry("stepper_z", { semin: null })]} />);
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
    render(<MotorTuningCard tuning={[x, y]} />);
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
    render(<MotorTuningCard tuning={[x, y]} />);
    expect(screen.getByRole("img", { name: "Some values differ from the other motor" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Show all values for stepper_y" }));
    expect(screen.queryByRole("img", { name: "Some values differ from the other motor" })).toBeNull();
    expect(screen.getByTitle("stepper_x: 1")).toBeTruthy();
  });

  it("shows no warning when nothing differs", () => {
    render(<MotorTuningCard tuning={[entry("stepper_x"), entry("stepper_y")]} />);
    expect(screen.queryByText(/have different configurations/)).toBeNull();
  });
});
