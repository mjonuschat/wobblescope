/// <reference types="node" />
import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

import { HALF_SPAN } from "./AxesMapForm";

describe("the calibration's start offset", () => {
  it("is_the_daemons_half_span", () => {
    const daemon = path.resolve(import.meta.dirname, "../../../../crates/daemon/src/capture/axes_map.rs");
    const found = /const HALF_SPAN: f64 = ([0-9.]+);/.exec(readFileSync(daemon, "utf8"));
    expect(found).not.toBeNull();
    expect(HALF_SPAN).toBe(Number(found?.[1]));
  });
});
