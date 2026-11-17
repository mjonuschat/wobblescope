import { describe, expect, it } from "vitest";

import { mountingLabel } from "./mounting";

describe("mountingLabel", () => {
  it("writes the known mountings in Title Case and leaves any other string alone", () => {
    expect(mountingLabel("toolhead")).toBe("Toolhead");
    expect(mountingLabel("bed")).toBe("Bed");
    expect(mountingLabel("gantry")).toBe("gantry");
  });
});
