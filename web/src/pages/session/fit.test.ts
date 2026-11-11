import { describe, expect, it } from "vitest";

import { twoAxisMeta } from "@/test/fixtures";

import { baselineFor, defaultFitParams, draftFrom } from "./fit";

const withPrinter = (over: Partial<NonNullable<ReturnType<typeof twoAxisMeta>["printer"]>>) => {
  const meta = twoAxisMeta();
  meta.printer = {
    name: null,
    klipper_version: null,
    mcu_firmware: [],
    square_corner_velocity: null,
    damping_ratio_x: null,
    damping_ratio_y: null,
    ...over,
  };
  return meta;
};

describe("baselineFor", () => {
  it("starts from Klipper's defaults when the session recorded nothing", () => {
    const baseline = baselineFor(twoAxisMeta(), "X");
    expect(baseline.params).toEqual(defaultFitParams);
    expect(baseline.recorded).toEqual({ scv: false, damping: false });
  });

  it("starts from the printer's corner velocity and the damping ratio of the axis", () => {
    const meta = withPrinter({ square_corner_velocity: 8, damping_ratio_x: 0.12, damping_ratio_y: 0.08 });
    expect(baselineFor(meta, "X").params).toMatchObject({ scv: 8, damping_ratio: 0.12 });
    expect(baselineFor(meta, "Y").params).toMatchObject({ scv: 8, damping_ratio: 0.08 });
    expect(baselineFor(meta, "Y").recorded).toEqual({ scv: true, damping: true });
  });

  it("keeps the default damping for an axis the printer sets none for", () => {
    const meta = withPrinter({ square_corner_velocity: 8, damping_ratio_x: 0.12 });
    expect(baselineFor(meta, "Y").params.damping_ratio).toBeNull();
    expect(baselineFor(meta, "Y").recorded).toEqual({ scv: true, damping: false });
  });
});

describe("draftFrom", () => {
  it("writes an unset value as an empty field", () => {
    expect(draftFrom({ ...defaultFitParams, scv: 8, damping_ratio: 0.12 })).toEqual({
      scv: "8",
      damping_ratio: "0.12",
      max_smoothing: "",
      max_vibrations: "",
    });
  });
});
