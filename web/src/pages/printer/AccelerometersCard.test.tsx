import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { chip, chipsSnapshot } from "@/test/capture";

import { AccelerometersCard } from "./AccelerometersCard";

function panel(over: Partial<React.ComponentProps<typeof AccelerometersCard>> = {}) {
  const props = {
    chips: chipsSnapshot(),
    onCheck: vi.fn(),
    checking: false,
    checkBlocked: null,
    ...over,
  };
  render(<AccelerometersCard {...props} />);
  return props;
}

describe("the accelerometers card", () => {
  it("lists each chip with its type, rate and availability", () => {
    panel({
      chips: chipsSnapshot({
        chips: [
          chip("adxl345", { availability: { state: "available", observed_rate_hz: 3198.4, checked_at: "x" } }),
          chip("lis2dw ebb36", {
            availability: { state: "unavailable", reason: "no valid samples arrived within 3 s" },
          }),
        ],
      }),
    });
    expect(screen.getByText("available · 3198 Hz")).toBeTruthy();
    expect(screen.getByText("unavailable")).toBeTruthy();
    expect(screen.getByText("no valid samples arrived within 3 s")).toBeTruthy();
  });

  it("names the axes each chip is the default for and the sections left out with their reasons", () => {
    panel({
      chips: chipsSnapshot({
        skipped: [{ section: "beacon cartographer", reason: "named Beacon devices are not supported yet" }],
      }),
    });
    expect(screen.getByText("X and Y")).toBeTruthy();
    expect(screen.getByText("none")).toBeTruthy();
    expect(screen.getByText("beacon cartographer: named Beacon devices are not supported yet")).toBeTruthy();
  });

  it("asks for a check, and cannot while a check runs or the printer is not ready", () => {
    const props = panel();
    fireEvent.click(screen.getByRole("button", { name: "Check chips" }));
    expect(props.onCheck).toHaveBeenCalledOnce();
  });

  it("disables the check with its reason when Klipper is not ready", () => {
    const props = panel({ checkBlocked: "Klipper is not ready." });
    const button = screen.getByRole("button", { name: "Check chips" }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(screen.getByText("Klipper is not ready.")).toBeTruthy();
    fireEvent.click(button);
    expect(props.onCheck).not.toHaveBeenCalled();
  });

  it("reports a list that is not current", () => {
    panel({ chips: chipsSnapshot({ current: false }) });
    expect(screen.getByText(/last list the daemon read/)).toBeTruthy();
  });
});
