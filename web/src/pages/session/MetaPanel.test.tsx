import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { twoAxisMeta } from "@/test/fixtures";

import { MetaPanel } from "./MetaPanel";

describe("the session details", () => {
  it("lists the firmware of each MCU and each chip's axes map", () => {
    const meta = twoAxisMeta();
    meta.printer = {
      name: "buster",
      klipper_version: "v0.13.0-820-g0cee841e",
      mcu_firmware: ["ebb36: v0.13.0-820-g0cee841e", "mcu: v0.13.0-658-gbd09e017"],
    };
    meta.chips[1].axes_map = "x,z,-y";
    render(<MetaPanel meta={meta} />);
    expect(screen.getByText("MCU firmware")).toBeTruthy();
    expect(screen.getByText("ebb36: v0.13.0-820-g0cee841e")).toBeTruthy();
    expect(screen.getByText("mcu: v0.13.0-658-gbd09e017")).toBeTruthy();
    const rows = screen.getAllByRole("row");
    expect(within(rows[2]).getByText("x,z,-y")).toBeTruthy();
    // A chip whose config sets no axes map shows a dash.
    expect(within(rows[1]).getAllByText("–").length).toBeGreaterThan(0);
  });

  it("shows no firmware row when none was recorded", () => {
    render(<MetaPanel meta={twoAxisMeta()} />);
    expect(screen.queryByText("MCU firmware")).toBeNull();
  });

  it("names the segments where a chip was degraded, even when its other segments were fine", () => {
    const meta = twoAxisMeta();
    meta.segments[1].files[0].health = "Degraded";
    meta.segments[1].files[0].health_notes = ["the chip's errors counter rose by 3"];
    render(<MetaPanel meta={meta} />);
    expect(screen.getByText("Degraded segments")).toBeTruthy();
    expect(screen.getByText(/Segment 2 \(Y, .*\): adxl345, the chip's errors counter rose by 3/)).toBeTruthy();
  });

  it("lists no degraded segment when every recording was fine or unjudged", () => {
    render(<MetaPanel meta={twoAxisMeta()} />);
    expect(screen.queryByText("Degraded segments")).toBeNull();
  });
});
