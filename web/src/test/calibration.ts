/// <reference types="node" />
import { readFileSync } from "node:fs";
import path from "node:path";

import type { Direction } from "@/lib/generated/Direction";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import type { WasmModule } from "@/analysis/handler";
import { config_snippet, initSync, WasmSession } from "../wasm/wobblescope_wasm.js";

const RATE_HZ = 3200;
const SAMPLES = 1920;
const PEAK = 80;

/** The wasm build, started synchronously so tests run the real analysis. */
export function realWasm(): WasmModule {
  initSync({ module: readFileSync(path.resolve(import.meta.dirname, "../wasm/wobblescope_wasm_bg.wasm")) });
  return { WasmSession, config_snippet } as unknown as WasmModule;
}

/** xxHash64 of `bytes` with seed 0; a zstd content checksum is its low 32 bits. */
function xxh64(bytes: Uint8Array): bigint {
  const mask = (1n << 64n) - 1n;
  const [p1, p2, p3, p4, p5] = [
    11400714785074694791n,
    14029467366897019727n,
    1609587929392839161n,
    9650029242287828579n,
    2870177450012600261n,
  ];
  const rotl = (x: bigint, r: bigint) => ((x << r) | (x >> (64n - r))) & mask;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const round = (acc: bigint, input: bigint) => (rotl((acc + input * p2) & mask, 31n) * p1) & mask;
  const merge = (acc: bigint, value: bigint) => ((((acc ^ round(0n, value)) * p1) & mask) + p4) & mask;
  let i = 0;
  let h: bigint;
  if (bytes.length >= 32) {
    let v = [(p1 + p2) & mask, p2, 0n, (0n - p1) & mask];
    for (; i + 32 <= bytes.length; i += 32) {
      v = v.map((acc, lane) => round(acc, view.getBigUint64(i + lane * 8, true)));
    }
    h = (rotl(v[0], 1n) + rotl(v[1], 7n) + rotl(v[2], 12n) + rotl(v[3], 18n)) & mask;
    for (const acc of v) h = merge(h, acc);
  } else {
    h = p5;
  }
  h = (h + BigInt(bytes.length)) & mask;
  for (; i + 8 <= bytes.length; i += 8) {
    h = (rotl(h ^ round(0n, view.getBigUint64(i, true)), 27n) * p1 + p4) & mask;
  }
  if (i + 4 <= bytes.length) {
    h = (rotl(h ^ ((BigInt(view.getUint32(i, true)) * p1) & mask), 23n) * p2 + p3) & mask;
    i += 4;
  }
  for (; i < bytes.length; i++) {
    h = (rotl(h ^ ((BigInt(bytes[i]) * p5) & mask), 11n) * p1) & mask;
  }
  h ^= h >> 33n;
  h = (h * p2) & mask;
  h ^= h >> 29n;
  h = (h * p3) & mask;
  return h ^ (h >> 32n);
}

/** One sample file: a trapezoid velocity move of `PEAK` mm/s on chip axis `axis`, gravity on Z, light noise. */
function sampleFile(axis: number): ArrayBuffer {
  const ramp = 0.1;
  const columns = [new Float64Array(SAMPLES), new Float64Array(SAMPLES), new Float64Array(SAMPLES).fill(9810)];
  let state = 12345;
  for (let i = 0; i < SAMPLES; i++) {
    const t = i / RATE_HZ;
    for (const column of columns) {
      state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
      column[i] += ((state >>> 16) / 65536 - 0.5) * 100;
    }
    const a = PEAK / ramp;
    if (t < ramp) columns[axis][i] += a;
    else if (t > 0.3 && t < 0.3 + ramp) columns[axis][i] -= a;
  }
  // The stored layout: magic, version, count, then the t, x, y, z columns.
  const payload = new Uint8Array(20 + SAMPLES * 32);
  const view = new DataView(payload.buffer);
  payload.set(new TextEncoder().encode("WSSAMPLE"));
  view.setUint32(8, 1, true);
  view.setBigUint64(12, BigInt(SAMPLES), true);
  const all = [Float64Array.from({ length: SAMPLES }, (_, i) => i / RATE_HZ), ...columns];
  for (const [c, column] of all.entries()) {
    for (const [i, v] of column.entries()) view.setFloat64(20 + (c * SAMPLES + i) * 8, v, true);
  }
  // One zstd frame: a single raw block and a content checksum.
  const frame = new Uint8Array(4 + 5 + 3 + payload.length + 4);
  const out = new DataView(frame.buffer);
  frame.set([0x28, 0xb5, 0x2f, 0xfd, 0xa4]);
  out.setUint32(5, payload.length, true);
  out.setUint32(9, (payload.length << 3) | 1, true);
  frame.set(payload, 12);
  out.setUint32(12 + payload.length, Number(xxh64(payload) & 0xffffffffn), true);
  return frame.buffer.slice(0, 12 + payload.length + 4);
}

/** Chip `a` sees the three moves on its X, Y and Z; chip `b` is turned, so it sees them on Y, X and Z. */
const CHIP_AXES = [
  [0, 1, 2],
  [1, 0, 2],
];

export const CALIBRATION_CHIPS = ["a", "b"];

/** The directions of the X, Y and Z moves, in segment order. */
const MOVES: Direction[] = [{ Vector: [1, 0, 0] }, { Vector: [0, 1, 0] }, { Vector: [0, 0, 1] }];

/** Three segments, two chips, one file per chip and segment. */
export function calibrationMeta(id = "cal1"): SessionMeta {
  return {
    format_version: 2,
    id,
    created_at: "2026-11-12T00:00:00Z",
    source: "Capture",
    time_domain: "KlipperPrintTime",
    mode: null,
    settings: null,
    printer: null,
    outcome: "Complete",
    axes_map_run: { speed: 80, accel: 3000, z_height: 30, travel_speed: 150, max_z_velocity: 100, max_z_accel: 3000 },
    notes: [],
    probe_points: [{ position: null }, { position: null }, { position: null }],
    chips: CALIBRATION_CHIPS.map((name) => ({
      name,
      chip_type: null,
      nominal_rate_hz: null,
      observed_rate_hz: null,
      axes_map: null,
      mounting: null,
      health: "Ok" as const,
      health_notes: [],
    })),
    segments: [0, 1, 2].map((segment) => ({
      axis: MOVES[segment],
      point: segment,
      window: null,
      files: CALIBRATION_CHIPS.map((_, chip) => ({
        chip,
        sample_file: `c${chip}-s${segment}.zst`,
        sample_count: SAMPLES,
        coverage: "Complete" as const,
        health: null,
        health_notes: [],
      })),
    })),
  };
}

/** The sample file of `chip` in `segment`. */
export function calibrationFile(chip: number, segment: number): ArrayBuffer {
  return sampleFile(CHIP_AXES[chip][segment]);
}
