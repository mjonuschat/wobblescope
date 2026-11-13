import { describe, expect, it } from "vitest";

import { axesMapSnippet } from "./axesMapSnippet";

describe("axesMapSnippet", () => {
  it("uses_axes_map_for_adxl_and_accel_axes_map_for_beacon", () => {
    expect(axesMapSnippet({ name: "adxl345", chip_type: "adxl345" }, "x, -z, y")).toBe(
      "[adxl345]\naxes_map: x, -z, y\n",
    );
    expect(axesMapSnippet({ name: "lis2dw", chip_type: "lis2dw" }, "x, y, z")).toBe("[lis2dw]\naxes_map: x, y, z\n");
    expect(axesMapSnippet({ name: "beacon", chip_type: "beacon" }, "-y, x, z")).toBe(
      "[beacon]\naccel_axes_map: -y, x, z\n",
    );
    expect(axesMapSnippet({ name: "beacon", chip_type: "Beacon" }, "x, y, z")).toBe(
      "[beacon]\naccel_axes_map: x, y, z\n",
    );
    expect(axesMapSnippet({ name: "adxl345", chip_type: null }, "x, y, z")).toBe("[adxl345]\naxes_map: x, y, z\n");
  });

  it("the_header_is_the_section_name_verbatim", () => {
    expect(axesMapSnippet({ name: "adxl345 nh36", chip_type: "adxl345" }, "x, -z, y").split("\n")[0]).toBe(
      "[adxl345 nh36]",
    );
    expect(axesMapSnippet({ name: "lis2dw bed", chip_type: "lis2dw" }, "x, -z, y").split("\n")[0]).toBe("[lis2dw bed]");
    expect(axesMapSnippet({ name: "beacon", chip_type: "beacon" }, "x, -z, y").split("\n")[0]).toBe("[beacon]");
  });
});
