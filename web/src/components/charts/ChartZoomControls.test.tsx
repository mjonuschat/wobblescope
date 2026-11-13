import { fireEvent, render, screen } from "@testing-library/react";
import type { ECharts } from "echarts/core";
import { describe, expect, it, vi } from "vitest";

import { ChartZoomControls } from "./ChartZoomControls";

function fakeChart() {
  const instance = { dispatchAction: vi.fn(), on: vi.fn(), off: vi.fn() };
  return { instance, ref: { current: instance as unknown as ECharts } };
}

describe("ChartZoomControls", () => {
  it("shows_both_labelled_buttons", () => {
    render(<ChartZoomControls chart={fakeChart().ref} />);
    expect(screen.getByRole("button", { name: "Zoom in" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Reset zoom" })).toBeTruthy();
  });

  it("zoom_in_arms_the_selection_box_and_a_second_click_disarms_it", () => {
    const { instance, ref } = fakeChart();
    render(<ChartZoomControls chart={ref} />);
    const zoomIn = screen.getByRole("button", { name: "Zoom in" });
    fireEvent.click(zoomIn);
    expect(instance.dispatchAction).toHaveBeenLastCalledWith({
      type: "takeGlobalCursor",
      key: "dataZoomSelect",
      dataZoomSelectActive: true,
    });
    expect(zoomIn.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(zoomIn);
    expect(instance.dispatchAction).toHaveBeenLastCalledWith(expect.objectContaining({ dataZoomSelectActive: false }));
    expect(zoomIn.getAttribute("aria-pressed")).toBe("false");
  });

  it("reset_zoom_restores_the_full_range", () => {
    const { instance, ref } = fakeChart();
    render(<ChartZoomControls chart={ref} />);
    fireEvent.click(screen.getByRole("button", { name: "Reset zoom" }));
    expect(instance.dispatchAction).toHaveBeenLastCalledWith({ type: "restore" });
  });

  it("are_native_focusable_buttons_so_the_keyboard_activates_them", () => {
    render(<ChartZoomControls chart={fakeChart().ref} />);
    for (const name of ["Zoom in", "Reset zoom"]) {
      const button = screen.getByRole("button", { name });
      expect(button.tagName).toBe("BUTTON");
      expect(button.getAttribute("type")).toBe("button");
      expect(button.getAttribute("tabindex")).not.toBe("-1");
      button.focus();
      expect(document.activeElement).toBe(button);
    }
  });
});
