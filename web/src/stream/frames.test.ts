import { describe, expect, it } from "vitest";

import { decodeFrame } from "./frames";

function build(index: number, t0: number, samples: number[]): ArrayBuffer {
  const buffer = new ArrayBuffer(16 + samples.length * 4);
  const view = new DataView(buffer);
  view.setUint16(0, index, true);
  view.setFloat64(4, t0, true);
  view.setUint32(12, samples.length / 3, true);
  samples.forEach((v, i) => {
    view.setFloat32(16 + i * 4, v, true);
  });
  return buffer;
}

describe("decodeFrame", () => {
  it("round-trips a frame", () => {
    const frame = decodeFrame(build(3, 12.5, [1, 2, 3, 4, 5, 6]));
    expect(frame.index).toBe(3);
    expect(frame.t0).toBe(12.5);
    expect(Array.from(frame.xyz)).toEqual([1, 2, 3, 4, 5, 6]);
  });

  it("throws on a truncated frame", () => {
    expect(() => decodeFrame(build(1, 0, [1, 2, 3, 4, 5, 6]).slice(0, 30))).toThrow();
    expect(() => decodeFrame(new ArrayBuffer(8))).toThrow();
  });
});
