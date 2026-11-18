import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { calibrationMeta } from "@/test/calibration";
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
    // Written as the Result card writes a map, with a space after each comma.
    expect(within(rows[2]).getByText("x, z, -y")).toBeTruthy();
    // A chip whose config sets no axes map shows a dash.
    expect(within(rows[1]).getAllByText("–").length).toBeGreaterThan(0);
  });

  it("gives the long Klipper version and the probe points two columns so they do not wrap", () => {
    const meta = twoAxisMeta();
    meta.printer = {
      name: "buster",
      klipper_version: "v0.13.0-821-gaf81d23f-dirty",
      mcu_firmware: [],
      square_corner_velocity: null,
      damping_ratio_x: null,
      damping_ratio_y: null,
    };
    meta.probe_points = [{ position: [135.5, 139, 20] }, { position: [165.5, 139, 20] }];
    render(<MetaPanel meta={meta} />);
    const cell = (label: string) => screen.getByText(label).parentElement as HTMLElement;
    expect(cell("Klipper").className).toContain("lg:col-span-3");
    expect(cell("Probe points").className).toContain("col-span-full");
    expect(cell("Printer").className).not.toContain("col-span");
  });

  it("groups the recording and the printer, each in its own grid", () => {
    const meta = twoAxisMeta();
    meta.printer = {
      name: "buster",
      klipper_version: "v0.13.0-821-gaf81d23f-dirty",
      mcu_firmware: [],
      square_corner_velocity: 5,
      damping_ratio_x: 0.059,
      damping_ratio_y: 0.123,
    };
    render(<MetaPanel meta={meta} />);
    const labelsOf = (name: string) =>
      Array.from(screen.getByLabelText(name).querySelectorAll("dt")).map((dt) => dt.textContent);
    expect(labelsOf("The recording")).toEqual(["Source", "Created", "Mode", "Outcome", "Probe points"]);
    expect(labelsOf("The printer")).toEqual([
      "Printer",
      "Klipper",
      "Square corner velocity",
      "Damping ratio X",
      "Damping ratio Y",
    ]);
  });

  it("shows the speed, acceleration and Z height a calibration ran with, and only for a calibration", () => {
    const { rerender } = render(<MetaPanel meta={calibrationMeta()} />);
    const settings = screen.getByLabelText("Calibration settings");
    const pairs = Array.from(settings.querySelectorAll("dt")).map((dt) => [
      dt.textContent,
      dt.nextElementSibling?.textContent,
    ]);
    expect(pairs).toEqual([
      ["Speed", "80 mm/s"],
      ["Acceleration", "3000 mm/s²"],
      ["Z height", "30 mm"],
    ]);
    rerender(<MetaPanel meta={twoAxisMeta()} />);
    expect(screen.queryByLabelText("Calibration settings")).toBeNull();
  });

  it("leaves out the input shaper's settings for an axes-map calibration", () => {
    const meta = calibrationMeta();
    meta.printer = {
      name: "buster",
      klipper_version: "v0.13.0",
      mcu_firmware: [],
      square_corner_velocity: 5,
      damping_ratio_x: 0.059,
      damping_ratio_y: 0.123,
    };
    render(<MetaPanel meta={meta} />);
    expect(screen.getByText("Klipper")).toBeTruthy();
    expect(screen.queryByText("Square corner velocity")).toBeNull();
    expect(screen.queryByText("Damping ratio X")).toBeNull();
    expect(screen.queryByText("Damping ratio Y")).toBeNull();
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
    expect(screen.queryByText("File name")).toBeNull();
  });

  it("names an import's source CSV and shows the file it came from", () => {
    const meta = twoAxisMeta();
    meta.source = { CsvImport: { original_file_name: "ws_x_adxl345.csv", original_file_names: ["ws_x_adxl345.csv"] } };
    render(<MetaPanel meta={meta} />);
    expect(screen.getByText("CSV")).toBeTruthy();
    expect(screen.getByText("File name")).toBeTruthy();
    expect(screen.getByText("ws_x_adxl345.csv")).toBeTruthy();
  });

  it("shows no file name for an import that has none", () => {
    const meta = twoAxisMeta();
    meta.source = { CsvImport: { original_file_name: null, original_file_names: [] } };
    render(<MetaPanel meta={meta} />);
    expect(screen.getByText("CSV")).toBeTruthy();
    expect(screen.queryByText("File name")).toBeNull();
  });

  it("names an axes-map calibration instead of a test mode", () => {
    render(<MetaPanel meta={calibrationMeta()} />);
    const mode = [...document.querySelectorAll("dt")].find((dt) => dt.textContent === "Mode");
    expect(mode?.nextElementSibling?.textContent).toBe("Axes map calibration");
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

  it("shows the direction of a belt segment instead of an object", () => {
    const meta = twoAxisMeta();
    meta.segments[0].axis = { Vector: [1, -1, 0] };
    meta.segments[0].files[0].health = "Degraded";
    meta.segments[0].files[0].health_notes = ["noisy"];
    meta.segments[1].files[0].health = "Degraded";
    meta.segments[1].files[0].health_notes = ["noisy"];
    meta.segments[1].axis = "X";
    render(<MetaPanel meta={meta} />);
    expect(screen.getByText(/Segment 1 \(1,-1, .*\)/)).toBeTruthy();
    expect(screen.getByText(/Segment 2 \(X, .*\)/)).toBeTruthy();
    expect(screen.queryByText(/object Object/)).toBeNull();
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

  it("keeps the outcome with the source, the time and the mode, and the probe points on a row of their own", () => {
    render(<MetaPanel meta={twoAxisMeta()} />);
    const labels = Array.from(screen.getByLabelText("The recording").querySelectorAll("dt")).map(
      (dt) => dt.textContent,
    );
    expect(labels.slice(0, 4)).toEqual(["Source", "Created", "Mode", "Outcome"]);
    expect(labels[labels.length - 1]).toBe("Probe points");
  });

  it("shows_the_vibrations_run_parameters", () => {
    const meta = twoAxisMeta();
    meta.vibrations_run = {
      size: 100,
      z_height: 20,
      max_speed: 100,
      speed_increment: 5,
      accel: 3000,
      travel_speed: 120,
      min_speed: 20,
      kinematics: "corexy",
    };
    render(<MetaPanel meta={meta} />);
    const group = screen.getByLabelText("Vibrations settings");
    const value = (label: string) => within(group).getByText(label).nextElementSibling?.textContent;
    expect(value("Size")).toBe("100 mm");
    expect(value("Z height")).toBe("20 mm");
    expect(value("Min speed")).toBe("20 mm/s");
    expect(value("Max speed")).toBe("100 mm/s");
    expect(value("Speed increment")).toBe("5 mm/s");
    expect(value("Acceleration")).toBe("3000 mm/s²");
    expect(value("Travel speed")).toBe("120 mm/s");
    expect(value("Kinematics")).toBe("corexy");
    expect(screen.getByText("Vibrations profile")).toBeTruthy();
  });

  it("hides_the_input_shaper_rows_for_a_vibrations_session", () => {
    const meta = twoAxisMeta();
    meta.printer = {
      name: "buster",
      klipper_version: "v0.13.0-821-gaf81d23f-dirty",
      mcu_firmware: [],
      square_corner_velocity: 5,
      damping_ratio_x: 0.059,
      damping_ratio_y: 0.123,
    };
    meta.vibrations_run = {
      size: 100,
      z_height: 20,
      max_speed: 100,
      speed_increment: 5,
      accel: 3000,
      travel_speed: 120,
      min_speed: 20,
      kinematics: "corexy",
    };
    render(<MetaPanel meta={meta} />);
    expect(screen.queryByText("Square corner velocity")).toBeNull();
    expect(screen.queryByText("Damping ratio X")).toBeNull();
    expect(screen.queryByText("Damping ratio Y")).toBeNull();
  });
});
