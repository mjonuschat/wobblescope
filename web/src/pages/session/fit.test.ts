import { describe, expect, it } from "vitest";

import { twoAxisMeta } from "@/test/fixtures";

import { baselineFor, defaultFitParams, draftFrom, draftToParams } from "./fit";

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
    expect(baseline.recorded).toEqual({ scv: false, damping: null });
  });

  it("starts from the printer's corner velocity and the damping ratio of the axis", () => {
    const meta = withPrinter({ square_corner_velocity: 8, damping_ratio_x: 0.12, damping_ratio_y: 0.08 });
    expect(baselineFor(meta, "X").params).toMatchObject({ scv: 8 });
    expect(baselineFor(meta, "Y").params).toMatchObject({ scv: 8 });
    expect(baselineFor(meta, "X").recorded).toEqual({ scv: true, damping: 0.12 });
    expect(baselineFor(meta, "Y").recorded).toEqual({ scv: true, damping: 0.08 });
  });

  it("leaves the damping ratio to the estimate even when the printer recorded one, as Shake&Tune does", () => {
    const meta = withPrinter({ damping_ratio_x: 0.12 });
    expect(baselineFor(meta, "X").params.damping_ratio).toBeNull();
  });

  it("estimates the damping ratio when the session recorded none", () => {
    expect(baselineFor(twoAxisMeta(), "X").params.damping_ratio).toBeNull();
  });

  it("estimates the damping ratio for an axis the printer sets none for", () => {
    const meta = withPrinter({ square_corner_velocity: 8, damping_ratio_x: 0.12 });
    expect(baselineFor(meta, "Y").params.damping_ratio).toBeNull();
    expect(baselineFor(meta, "Y").recorded).toEqual({ scv: true, damping: null });
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

describe("the vibration limit", () => {
  it("is a percentage on the page and a fraction in the analysis", () => {
    expect(draftFrom({ ...defaultFitParams, max_vibrations: 0.05 }).max_vibrations).toBe("5");
    expect(draftFrom({ ...defaultFitParams, max_vibrations: 0.07 }).max_vibrations).toBe("7");
    const draft = { ...draftFrom(defaultFitParams), max_vibrations: "5" };
    const result = draftToParams(draft);
    expect("params" in result && result.params.max_vibrations).toBe(0.05);
  });

  it("is unset when the field is empty", () => {
    const result = draftToParams(draftFrom(defaultFitParams));
    expect("params" in result && result.params.max_vibrations).toBeNull();
  });
});
