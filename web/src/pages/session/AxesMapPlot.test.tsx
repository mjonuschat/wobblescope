import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { AxesMapResult } from "@/lib/generated/AxesMapResult";
import type { AxisMatch } from "@/lib/generated/AxisMatch";

import { AxesMapPlot, isometric, movementOption, movementZones, orientationArrows } from "./AxesMapPlot";

vi.mock("@/components/charts/EChart", () => ({
  EChart: ({ label }: { label: string }) => <div role="img" aria-label={label} />,
}));

function axis(over: Partial<AxisMatch> = {}): AxisMatch {
  return {
    state: "measured",
    chip_axis: "X",
    negative: false,
    confidence: 0.87,
    tilt_deg: 2.34,
    peaks: [80, 2, 1],
    peak_velocity: 80,
    peak_time_s: 0.1,
    ...over,
  };
}

/** A chip whose y points along machine Z and whose z points along machine -Y; machine Z is virtual. */
function result(over: Partial<AxesMapResult> = {}): AxesMapResult {
  const segment = (k: number) => ({
    t: [0, 0.1, 0.2],
    velocity: [0, 1, 2].map((a) => (a === k ? [0, 80, 0] : [0, 1, 0])) as [number[], number[], number[]],
  });
  return {
    chip: "adxl345",
    state: "complete",
    map: "x, -z, y",
    configured_map: null,
    axes: [
      axis({ chip_axis: "X", peak_velocity: 80, peak_time_s: 0.1 }),
      axis({
        chip_axis: "Z",
        negative: true,
        confidence: 0.5,
        tilt_deg: 1.04,
        peak_velocity: -79.6,
        peak_time_s: 0.12,
      }),
      axis({ state: "virtual", chip_axis: "Y", confidence: 0, tilt_deg: 0, peaks: [0, 0, 0], peak_velocity: 0 }),
    ],
    euler_deg: [90, 0, 0],
    rotation: [
      [1, 0, 0],
      [0, 0, -1],
      [0, 1, 0],
    ],
    gravity: 9.81234,
    noise: 412.4,
    noise_verdict: "High",
    quality: { status: "Ok", messages: [] },
    config_check: "does_not_match",
    curves: [segment(0), segment(2), segment(1)],
    core_version: "0.1.0",
    ...over,
  };
}

describe("the movement sequence", () => {
  it("joins_the_three_segments_end_to_end", () => {
    const zones = movementZones(result());
    expect(zones.map((z) => [z.axis, z.start, z.end])).toEqual([
      ["X", 0, 0.2],
      ["Y", 0.2, 0.4],
      ["Z", 0.4, 0.6000000000000001],
    ]);
    const series = (movementOption(result()) as { series: { name: string; color: string; data: number[][] }[] }).series;
    expect(series.map((s) => [s.name, s.color])).toEqual([
      ["Chip x", "#70088C"],
      ["Chip y", "#FF8D32"],
      ["Chip z", "#F2055C"],
    ]);
    expect(series[0].data.map(([t]) => t)).toEqual([
      0, 0.1, 0.2, 0.2, 0.30000000000000004, 0.4, 0.4, 0.5, 0.6000000000000001,
    ]);
    expect(series[0].data.map(([, v]) => v)).toEqual([0, 80, 0, 0, 1, 0, 0, 1, 0]);
  });

  it("labels_each_zone_with_its_letter_and_confidence_or_the_inferred_note", () => {
    const zones = movementZones(result());
    expect(zones.map((z) => [z.axis, z.virtual, z.note])).toEqual([
      ["X", false, "Confidence: 87%"],
      ["Y", false, "Confidence: 50%"],
      ["Z", true, "Inferred (no signal on this axis)"],
    ]);
  });

  it("marks_each_detected_peak_at_its_time_with_its_value", () => {
    const zones = movementZones(result());
    expect(zones.map((z) => z.peak)).toEqual([
      { chipAxis: 0, time: 0.1, value: 80, label: "80 mm/s" },
      { chipAxis: 2, time: 0.32, value: -79.6, label: "-80 mm/s" },
      null,
    ]);
  });

  it("gives_a_not_determined_zone_no_confidence_and_no_peak", () => {
    const undetermined = axis({ state: "not_determined", chip_axis: null, confidence: 0, peak_velocity: 0 });
    const zones = movementZones(result({ state: "partial", axes: [axis(), undetermined, undetermined] }));
    expect(zones.map((z) => [z.note, z.peak === null])).toEqual([
      ["Confidence: 87%", false],
      ["Not determined", true],
      ["Not determined", true],
    ]);
  });

  it("draws_dashed_separators_and_a_grey_zone_for_the_virtual_axis", () => {
    const series = (movementOption(result()) as { series: Record<string, unknown>[] }).series;
    const markLine = series[0].markLine as { lineStyle: { type: string }; data: { xAxis: number }[] };
    expect(markLine.lineStyle.type).toBe("dashed");
    expect(markLine.data.map((d) => d.xAxis)).toEqual([0.2, 0.4]);
    const markArea = series[0].markArea as {
      data: [{ xAxis: number; itemStyle: { color: string }; label: { formatter: string } }, { xAxis: number }][];
    };
    expect(markArea.data.map(([from]) => from.label.formatter)).toEqual([
      "{letter|X}\n{note|Confidence: 87%}",
      "{letter|Y}\n{note|Confidence: 50%}",
      "{letter|Z}\n{note|Inferred\n(no signal on this axis)}",
    ]);
    expect(markArea.data[2][0].itemStyle.color).not.toBe(markArea.data[0][0].itemStyle.color);
  });
});

describe("the orientation view", () => {
  it("puts_the_velocity_label_left_of_the_values_turned_a_quarter", () => {
    const option = movementOption(result()) as unknown as {
      yAxis: { name: string; nameLocation: string; nameRotate: number; nameGap: number };
      grid: { left: number };
    };
    expect(option.yAxis).toMatchObject({ name: "Velocity (mm/s)", nameLocation: "middle", nameRotate: 90 });
    // The grid leaves room for the values and then the label.
    expect(option.grid.left).toBeGreaterThan(option.yAxis.nameGap);
  });

  it("pads_the_velocity_axis_by_15_percent_of_the_data_range_so_peak_labels_clear_the_edge", () => {
    const { yAxis } = movementOption(result()) as unknown as {
      yAxis: { min: (e: { min: number; max: number }) => number; max: (e: { min: number; max: number }) => number };
    };
    const extent = { min: -90, max: 70 };
    // Shake&Tune pads by 15 % of the range at each end.
    expect(yAxis.min(extent)).toBeCloseTo(-90 - 0.15 * 160);
    expect(yAxis.max(extent)).toBeCloseTo(70 + 0.15 * 160);
    // The padded ends are not round numbers and are not labelled.
    const label = (movementOption(result()) as unknown as { yAxis: { axisLabel: object } }).yAxis.axisLabel;
    expect(label).toMatchObject({ showMinLabel: false, showMaxLabel: false });
    // A flat recording still gets an axis with some height.
    const flat = { min: 0, max: 0 };
    expect(yAxis.max(flat)).toBeGreaterThan(yAxis.min(flat));
  });

  it("projects_machine_axes_isometrically", () => {
    const [zx, zy] = isometric([0, 0, 1]);
    expect(zx).toBeCloseTo(0);
    expect(zy).toBeLessThan(0);
    const [xx, xy] = isometric([1, 0, 0]);
    const [yx, yy] = isometric([0, 1, 0]);
    // X points to the lower right, Y to the upper right (into the picture), Z straight up.
    expect(xx).toBeGreaterThan(0);
    expect(xy).toBeGreaterThan(0);
    expect(yx).toBeGreaterThan(0);
    expect(yy).toBeLessThan(0);
    // A right-handed frame seen from outside: X, Y and Z all have the same length on screen.
    expect(Math.hypot(xx, xy)).toBeCloseTo(Math.hypot(zx, zy));
    expect(Math.hypot(yx, yy)).toBeCloseTo(Math.hypot(zx, zy));
  });

  it("points_each_chip_arrow_along_its_rotation_column", () => {
    const arrows = orientationArrows(result())!;
    expect(arrows.machine.map((a) => a.label)).toEqual(["X", "Y", "Z"]);
    expect(arrows.chip.map((a) => [a.label, a.color])).toEqual([
      ["X", "#70088C"],
      ["Y", "#FF8D32"],
      ["Z", "#F2055C"],
    ]);
    // Column 1 of the rotation is chip y: machine (0, 0, 1). Column 2 is chip z: machine (0, -1, 0).
    const [y1, y2] = isometric([0, 0, 0.9]);
    expect(arrows.chip[1].end[0]).toBeCloseTo(y1);
    expect(arrows.chip[1].end[1]).toBeCloseTo(y2);
    const [z1, z2] = isometric([0, -0.9, 0]);
    expect(arrows.chip[2].end[0]).toBeCloseTo(z1);
    expect(arrows.chip[2].end[1]).toBeCloseTo(z2);
  });

  it("shows_nothing_without_a_rotation", () => {
    expect(orientationArrows(result({ rotation: null }))).toBeNull();
  });

  it("titles_both_graphs_and_labels_both_sets_of_axes_in_capitals", async () => {
    render(<AxesMapPlot result={result()} />);
    expect(screen.getByRole("heading", { name: "Movement sequence" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Chip orientation" })).toBeTruthy();
    // The key is a tooltip on an (i) button beside the title, not text on the page.
    expect(screen.queryByText(/Dashed: printer axes/)).toBeNull();
    fireEvent.focus(screen.getByRole("button", { name: "About chip orientation" }));
    expect(
      (await screen.findAllByText(/Dashed: printer axes\. Coloured: chip axes, as mounted\./)).length,
    ).toBeGreaterThan(0);
    // The machine and the chip axes are each X, Y and Z, told apart by colour.
    for (const letter of ["X", "Y", "Z"]) expect(screen.getAllByText(letter)).toHaveLength(2);
    expect(screen.queryByText(/inferred/i)).toBeNull();
    expect(screen.queryByText("y")).toBeNull();
    // The map, the axes and the Euler angles are in the Result card and the table, not repeated here.
    expect(screen.queryByText(/Detected axes map/)).toBeNull();
    // Gravity and noise are in the chip panel's fields, not repeated on the chart.
    expect(screen.queryByText(/Measured gravity/)).toBeNull();
    expect(screen.queryByText(/^Noise:/)).toBeNull();
    expect(screen.getByRole("img", { name: /movement sequence/i })).toBeTruthy();
  });

  it("draws_no_orientation_without_a_rotation", () => {
    render(<AxesMapPlot result={result({ rotation: null })} />);
    expect(screen.queryByRole("heading", { name: "Chip orientation" })).toBeNull();
    expect(screen.queryByRole("img", { name: /orientation/i })).toBeNull();
    expect(screen.getByRole("heading", { name: "Movement sequence" })).toBeTruthy();
  });
});
