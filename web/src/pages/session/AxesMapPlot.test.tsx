import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { AxesMapResult } from "@/lib/generated/AxesMapResult";
import type { AxisMatch } from "@/lib/generated/AxisMatch";

import {
  AxesMapPlot,
  gravityText,
  isometric,
  movementOption,
  movementZones,
  noiseText,
  orientationArrows,
  orientationHeader,
} from "./AxesMapPlot";

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
    config_check: "none_configured",
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

  it("labels_each_zone_with_its_letter_and_confidence_or_the_extrapolated_note", () => {
    const zones = movementZones(result());
    expect(zones.map((z) => [z.axis, z.virtual, z.note])).toEqual([
      ["X", false, "Confidence: 87%"],
      ["Y", false, "Confidence: 50%"],
      ["Z", true, "Extrapolated (no signal on this axis)"],
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
      "{letter|Z}\n{note|Extrapolated (no signal on this axis)}",
    ]);
    expect(markArea.data[2][0].itemStyle.color).not.toBe(markArea.data[0][0].itemStyle.color);
  });

  it("states_gravity_and_the_noise_verdict", () => {
    expect(gravityText(result())).toBe("Measured gravity: 9.812 m/s²");
    expect(noiseText(result())).toBe("Noise: 412 mm/s² (High)");
    expect(noiseText(result({ noise: 120, noise_verdict: "Fine" }))).toBe("Noise: 120 mm/s² (OK)");
    expect(noiseText(result({ noise: 900, noise_verdict: "TooHigh" }))).toBe("Noise: 900 mm/s² (Too high)");
  });
});

describe("the orientation view", () => {
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
      ["x", "#70088C"],
      ["y (virtual)", "#FF8D32"],
      ["z", "#F2055C"],
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

  it("lists_the_map_each_axis_and_the_euler_angles", () => {
    expect(orientationHeader(result())).toEqual({
      map: "Detected axes map: x, -z, y",
      axes: ["X -> x (angle error: 2.3°)", "Y -> -z (angle error: 1.0°)", "Z -> y (extrapolated)"],
      euler: "Euler orientation: X=90.0° Y=0.0° Z=0.0°",
    });
    const undetermined = axis({ state: "not_determined", chip_axis: null });
    expect(
      orientationHeader(result({ map: null, euler_deg: null, axes: [axis(), undetermined, undetermined] })),
    ).toEqual({
      map: "Detected axes map: none",
      axes: ["X -> x (angle error: 2.3°)", "Y: not determined", "Z: not determined"],
      euler: null,
    });
  });

  it("draws_the_virtual_label_and_the_zone_labels_on_screen", () => {
    render(<AxesMapPlot result={result()} />);
    expect(screen.getByText("y (virtual)")).toBeTruthy();
    expect(screen.getByText("Detected axes map: x, -z, y")).toBeTruthy();
    expect(screen.getByText("Z -> y (extrapolated)")).toBeTruthy();
    expect(screen.getByText("Measured gravity: 9.812 m/s²")).toBeTruthy();
    expect(screen.getByRole("img", { name: /movement sequence/i })).toBeTruthy();
  });

  it("draws_no_orientation_without_a_rotation", () => {
    render(<AxesMapPlot result={result({ rotation: null })} />);
    expect(screen.queryByText("y (virtual)")).toBeNull();
    expect(screen.queryByRole("img", { name: /orientation/i })).toBeNull();
  });
});
