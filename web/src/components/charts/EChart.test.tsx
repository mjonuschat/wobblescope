import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { EChart } from "./EChart";

const instance = vi.hoisted(() => ({
  getZr: () => ({ on: vi.fn(), off: vi.fn() }),
  setOption: vi.fn(),
  getOption: vi.fn(() => ({ legend: [] as { selected?: Record<string, boolean> }[] })),
  resize: vi.fn(),
  dispose: vi.fn(),
  dispatchAction: vi.fn(),
  on: vi.fn(),
  off: vi.fn(),
}));
vi.mock("echarts/core", () => ({ init: () => instance, use: vi.fn() }));
vi.mock("./register", () => ({}));

describe("EChart", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
  });

  it("renders_no_zoom_buttons_by_default", () => {
    render(<EChart option={{}} label="plain" />);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("renders_the_zoom_buttons_when_asked", () => {
    render(<EChart option={{}} label="zoomable" zoom />);
    expect(screen.getByRole("button", { name: "Zoom in" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Reset zoom" })).toBeTruthy();
  });

  describe("legend switches", () => {
    const option = {
      legend: [
        { data: ["Total", "X", "Y"], top: 0 },
        { data: ["zv", "mzv"], top: 28 },
      ],
    };

    it("shows one switch per legend, on only when every curve of it is shown", () => {
      instance.getOption.mockReturnValue({ legend: [{ selected: { X: false, Y: false } }, { selected: {} }] });
      render(<EChart option={option} label="x" legendSwitches={["Axes", "Shapers"]} />);
      expect(screen.getByRole("switch", { name: "Show all axes" }).getAttribute("aria-checked")).toBe("false");
      expect(screen.getByRole("switch", { name: "Show all shapers" }).getAttribute("aria-checked")).toBe("true");
    });

    it("counts a row with some curves hidden as off", () => {
      instance.getOption.mockReturnValue({ legend: [{}, { selected: { mzv: false } }] });
      render(<EChart option={option} label="x" legendSwitches={["Axes", "Shapers"]} />);
      expect(screen.getByRole("switch", { name: "Show all shapers" }).getAttribute("aria-checked")).toBe("false");
    });

    it("shows every curve of that legend when switched on", () => {
      instance.dispatchAction.mockClear();
      instance.getOption.mockReturnValue({ legend: [{ selected: { X: false } }, {}] });
      render(<EChart option={option} label="x" legendSwitches={["Axes", "Shapers"]} />);
      fireEvent.click(screen.getByRole("switch", { name: "Show all axes" }));
      expect(instance.dispatchAction).toHaveBeenCalledWith({ type: "legendAllSelect", legendIndex: 0 });
    });

    it("hides each curve of that legend when switched off", () => {
      instance.dispatchAction.mockClear();
      instance.getOption.mockReturnValue({ legend: [{}, {}] });
      render(<EChart option={option} label="x" legendSwitches={["Axes", "Shapers"]} />);
      fireEvent.click(screen.getByRole("switch", { name: "Show all shapers" }));
      expect(instance.dispatchAction.mock.calls.map((c) => c[0])).toEqual([
        { type: "legendUnSelect", legendIndex: 1, name: "zv" },
        { type: "legendUnSelect", legendIndex: 1, name: "mzv" },
      ]);
    });

    it("has no switches unless asked", () => {
      render(<EChart option={option} label="x" />);
      expect(screen.queryByRole("switch")).toBeNull();
    });
  });

  describe("legend choices", () => {
    it("keeps what the chart holds when the option is replaced", () => {
      instance.setOption.mockClear();
      instance.getOption.mockReturnValue({ legend: [{ selected: { X: true, Z: false } }] });
      render(<EChart option={{ legend: { data: ["X", "Z"], selected: { X: false } } }} label="x" />);
      const sent = instance.setOption.mock.calls.at(-1)?.[0] as { legend: { selected: Record<string, boolean> } };
      expect(sent.legend.selected).toEqual({ X: true, Z: false });
    });

    it("drops a choice about a curve the new legend no longer lists", () => {
      instance.setOption.mockClear();
      instance.getOption.mockReturnValue({ legend: [{ selected: { X: false, "3hump_ei": false } }] });
      render(<EChart option={{ legend: { data: ["X"] } }} label="x" />);
      const sent = instance.setOption.mock.calls.at(-1)?.[0] as { legend: { selected: Record<string, boolean> } };
      expect(sent.legend.selected).toEqual({ X: false });
    });

    it("sends an option as it is while the chart holds no legend yet", () => {
      instance.setOption.mockClear();
      instance.getOption.mockReturnValue({ legend: [] });
      const option = { legend: { data: ["X"], selected: { X: false } } };
      render(<EChart option={option} label="x" />);
      expect(instance.setOption.mock.calls.at(-1)?.[0]).toBe(option);
    });
  });
});
