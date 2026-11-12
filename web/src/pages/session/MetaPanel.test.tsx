import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { twoAxisMeta } from "@/test/fixtures";

import { MetaPanel } from "./MetaPanel";

describe("the session details", () => {
  it("shows each chip's axes map and not the MCU firmware", () => {
    const meta = twoAxisMeta();
    meta.printer = {
      name: "buster",
      klipper_version: "v0.13.0-820-g0cee841e",
      mcu_firmware: ["ebb36: v0.13.0-820-g0cee841e", "mcu: v0.13.0-658-gbd09e017"],
      square_corner_velocity: null,
      damping_ratio_x: null,
      damping_ratio_y: null,
    };
    meta.chips[1].axes_map = "x,z,-y";
    render(<MetaPanel meta={meta} />);
    expect(screen.queryByText("MCU firmware")).toBeNull();
    expect(screen.queryByText("mcu: v0.13.0-658-gbd09e017")).toBeNull();
    const rows = screen.getAllByRole("row");
    expect(within(rows[2]).getByText("x,z,-y")).toBeTruthy();
    // A chip whose config sets no axes map shows a dash.
    expect(within(rows[1]).getAllByText("–").length).toBeGreaterThan(0);
  });

  it("shows the printer's corner velocity and damping ratios when they were recorded", () => {
    const meta = twoAxisMeta();
    meta.printer = {
      name: "buster",
      klipper_version: null,
      mcu_firmware: [],
      square_corner_velocity: 8,
      damping_ratio_x: 0.12,
      damping_ratio_y: null,
    };
    render(<MetaPanel meta={meta} />);
    expect(screen.getByText("8 mm/s")).toBeTruthy();
    expect(screen.getByText("Damping ratio X")).toBeTruthy();
    expect(screen.queryByText("Damping ratio Y")).toBeNull();
  });

  it("names a test's source Wobblescope and shows no file name", () => {
    render(<MetaPanel meta={twoAxisMeta()} />);
    expect(screen.getByText("Wobblescope")).toBeTruthy();
    expect(screen.queryByText("File Name")).toBeNull();
  });

  it("names an import's source CSV and shows the file it came from", () => {
    const meta = twoAxisMeta();
    meta.source = { CsvImport: { original_file_name: "ws_x_adxl345.csv", original_file_names: ["ws_x_adxl345.csv"] } };
    render(<MetaPanel meta={meta} />);
    expect(screen.getByText("CSV")).toBeTruthy();
    expect(screen.getByText("File Name")).toBeTruthy();
    expect(screen.getByText("ws_x_adxl345.csv")).toBeTruthy();
  });

  it("shows no file name for an import that has none", () => {
    const meta = twoAxisMeta();
    meta.source = { CsvImport: { original_file_name: null, original_file_names: [] } };
    render(<MetaPanel meta={meta} />);
    expect(screen.getByText("CSV")).toBeTruthy();
    expect(screen.queryByText("File Name")).toBeNull();
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

  it("lists no degraded segment when every recording was fine", () => {
    render(<MetaPanel meta={twoAxisMeta()} />);
    expect(screen.queryByText("Degraded segments")).toBeNull();
  });

  describe("the probe points", () => {
    const withPoints = (positions: ([number, number, number] | null)[]) => {
      const meta = twoAxisMeta();
      meta.probe_points = positions.map((position) => ({ position }));
      return meta;
    };

    it("shows a single point as X, Y and Z with no expand button", () => {
      render(<MetaPanel meta={withPoints([[150.5, 154, 50]])} />);
      expect(screen.getByText("X: 150.5, Y: 154, Z: 50")).toBeTruthy();
      expect(screen.queryByRole("button", { name: /points/ })).toBeNull();
    });

    it("shows the first point and a button that opens every point, one per line", () => {
      render(
        <MetaPanel
          meta={withPoints([
            [20, 20, 20],
            [150.5, 154, 20],
            [280, 288, 20],
          ])}
        />,
      );
      expect(screen.getByText("X: 20, Y: 20, Z: 20")).toBeTruthy();
      expect(screen.queryByText("X: 150.5, Y: 154, Z: 20")).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: "Show all 3 points" }));
      const lines = screen.getAllByRole("listitem").map((li) => li.textContent);
      expect(lines).toEqual(["1 X: 20, Y: 20, Z: 20", "2 X: 150.5, Y: 154, Z: 20", "3 X: 280, Y: 288, Z: 20"]);
      fireEvent.click(screen.getByRole("button", { name: "Show only the first point" }));
      expect(screen.queryByRole("listitem")).toBeNull();
    });

    it("says so when a point's position was not recorded", () => {
      render(<MetaPanel meta={withPoints([null])} />);
      expect(screen.getAllByText("Not recorded").length).toBeGreaterThan(0);
    });
  });

  it("puts the outcome last among the recording's facts", () => {
    render(<MetaPanel meta={twoAxisMeta()} />);
    const labels = [...document.querySelectorAll("dt")].map((dt) => dt.textContent);
    const first = labels.indexOf("Source");
    expect(labels.slice(first).indexOf("Outcome")).toBeGreaterThan(labels.slice(first).indexOf("Probe Points"));
  });
});
