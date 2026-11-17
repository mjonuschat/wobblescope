import { describe, expect, it } from "vitest";

import { offersVibrations } from "./kinds";

describe("offersVibrations", () => {
  it.each([
    ["cartesian", true],
    ["limited_cartesian", true],
    ["corexy", true],
    ["limited_corexy", true],
    ["corexz", true],
    ["limited_corexz", true],
    ["delta", false],
    ["", false],
    [null, false],
    [undefined, false],
  ])("%s is %s", (kinematics, offered) => {
    expect(offersVibrations(kinematics)).toBe(offered);
  });
});
