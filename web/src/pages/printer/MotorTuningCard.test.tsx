import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { MotorTuning } from "@/lib/generated/MotorTuning";

import { MotorTuningCard } from "./MotorTuningCard";

function entry(stepper: string, over: Partial<MotorTuning> = {}): MotorTuning {
  return {
    stepper,
    driver: "tmc2240",
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
    ...over,
  };
}

describe("the motor tuning card", () => {
  it("renders nothing when there are no autotune sections", () => {
    const { container } = render(<MotorTuningCard tuning={[]} />);
    expect(container.textContent).toBe("");
  });

  it("shows one row per stepper", () => {
    render(<MotorTuningCard tuning={[entry("stepper_z"), entry("extruder", { driver: null, voltage: null })]} />);
    expect(screen.getByText("Motor tuning")).toBeTruthy();
    const rows = screen.getAllByRole("row");
    expect(rows).toHaveLength(3);
    expect(within(rows[1]).getByText("stepper_z")).toBeTruthy();
    expect(within(rows[1]).getByText("24 V")).toBeTruthy();
    expect(within(rows[2]).getAllByText("–")).toHaveLength(2);
  });

  it("shows the options that are set when a row is opened, and hides the rest", () => {
    render(<MotorTuningCard tuning={[entry("stepper_z")]} />);
    expect(screen.queryByText("pwm_freq_target")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show all values for stepper_z" }));
    expect(screen.getByText("pwm_freq_target")).toBeTruthy();
    expect(screen.getByText("20000")).toBeTruthy();
    expect(screen.queryByText("tpfd")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Hide all values for stepper_z" }));
    expect(screen.queryByText("pwm_freq_target")).toBeNull();
  });
});
