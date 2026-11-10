import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { FitSettingsForm } from "./FitSettingsForm";

const field = (label: RegExp) => screen.getByLabelText(label);

describe("FitSettingsForm", () => {
  it("applies Klipper\u2019s defaults when every optional field is empty", () => {
    const onApply = vi.fn();
    render(<FitSettingsForm onApply={onApply} />);
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(onApply).toHaveBeenCalledWith({
      scv: 5,
      damping_ratio: null,
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
    fireEvent.change(field(/Smoothing limit/), { target: { value: "0.2" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(onApply).toHaveBeenLastCalledWith(expect.objectContaining({ max_smoothing: 0.2 }));
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(onApply).toHaveBeenLastCalledWith(expect.objectContaining({ scv: 5, max_smoothing: null }));
    expect((field(/Smoothing limit/) as HTMLInputElement).value).toBe("");
  });

  it("disables Apply and marks the field when it holds text that is not a number", () => {
    const onApply = vi.fn();
    render(<FitSettingsForm onApply={onApply} />);
    fireEvent.change(field(/Damping ratio/), { target: { value: "abc" } });
    expect((screen.getByRole("button", { name: "Apply" }) as HTMLButtonElement).disabled).toBe(true);
    expect(field(/Damping ratio/).getAttribute("aria-invalid")).toBe("true");
    expect(onApply).not.toHaveBeenCalled();
  });
});
