import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { FitSettingsForm } from "./FitSettingsForm";
import { defaultFitParams } from "./fit";

const field = (label: RegExp) => screen.getByLabelText(label);

describe("FitSettingsForm", () => {
  it("applies Klipper\u2019s defaults, with the damping ratio filled in as 0.1, when nothing is changed", () => {
    const onApply = vi.fn();
    render(<FitSettingsForm onApply={onApply} />);
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(onApply).toHaveBeenCalledWith({
      scv: 5,
      damping_ratio: 0.1,
      max_smoothing: null,
      max_vibrations: null,
      test_damping_ratios: null,
      max_freq: null,
      shapers: null,
    });
  });

  it("applies a smoothing limit, and Reset applies the defaults again", () => {
    const onApply = vi.fn();
    render(<FitSettingsForm onApply={onApply} />);
    fireEvent.change(field(/Maximum Corner Rounding/), { target: { value: "0.2" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(onApply).toHaveBeenLastCalledWith(expect.objectContaining({ max_smoothing: 0.2 }));
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(onApply).toHaveBeenLastCalledWith(expect.objectContaining({ scv: 5, max_smoothing: null }));
    expect((field(/Maximum Corner Rounding/) as HTMLInputElement).value).toBe("");
  });

  it("starts from the printer's recorded values, says so, and Reset goes back to them", () => {
    const onApply = vi.fn();
    const baseline = {
      params: { ...defaultFitParams, scv: 8, damping_ratio: 0.12 },
      recorded: { scv: true, damping: true },
    };
    render(<FitSettingsForm baseline={baseline} onApply={onApply} />);
    expect((field(/Square Corner Velocity/) as HTMLInputElement).value).toBe("8");
    expect((field(/Damping Ratio/) as HTMLInputElement).value).toBe("0.12");
    expect(screen.getByText(/From the printer when it was tested\./)).toBeTruthy();
    fireEvent.change(field(/Square Corner Velocity/), { target: { value: "3" } });
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect((field(/Square Corner Velocity/) as HTMLInputElement).value).toBe("8");
    expect(onApply).toHaveBeenLastCalledWith(expect.objectContaining({ scv: 8, damping_ratio: 0.12 }));
  });

  it("fills the damping ratio with 0.1 when the printer set none, and an empty field still means 0.1", () => {
    const onApply = vi.fn();
    render(<FitSettingsForm onApply={onApply} />);
    expect((field(/Damping Ratio/) as HTMLInputElement).value).toBe("0.1");
    fireEvent.change(field(/Damping Ratio/), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(onApply).toHaveBeenCalledWith(expect.objectContaining({ damping_ratio: null }));
  });

  it("says Klipper's default is used when nothing was recorded", () => {
    render(<FitSettingsForm onApply={vi.fn()} />);
    expect(screen.getByText(/Not recorded for this session\./)).toBeTruthy();
  });

  it("lets a parent handle Reset", () => {
    const onApply = vi.fn();
    const onReset = vi.fn();
    render(<FitSettingsForm onApply={onApply} onReset={onReset} />);
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(onReset).toHaveBeenCalledOnce();
    expect(onApply).not.toHaveBeenCalled();
  });

  it("takes the vibration limit as a percentage", () => {
    const onApply = vi.fn();
    render(<FitSettingsForm onApply={onApply} />);
    fireEvent.change(field(/Acceptable Vibration/), { target: { value: "5" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(onApply).toHaveBeenCalledWith(expect.objectContaining({ max_vibrations: 0.05 }));
  });
});
