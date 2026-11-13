import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { EChart } from "./EChart";

vi.mock("echarts/core", () => {
  const instance = {
    getZr: () => ({ on: vi.fn(), off: vi.fn() }),
    setOption: vi.fn(),
    resize: vi.fn(),
    dispose: vi.fn(),
    dispatchAction: vi.fn(),
    on: vi.fn(),
    off: vi.fn(),
  };
  return { init: () => instance, use: vi.fn() };
});
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
});
