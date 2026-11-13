import { describe, expect, it } from "vitest";

import { analysis, candidate, defaultParams, spectrogram, twoAxisMeta } from "@/test/fixtures";
import type { Analysis } from "@/lib/generated/Analysis";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import type { Spectrogram } from "@/lib/generated/Spectrogram";

import { createHandler, type SessionHandle, type WasmModule } from "./handler";
import type { AnalyzeRequest, SpectrogramRequest } from "./protocol";

class FakeSession implements SessionHandle {
  files = new Map<string, Uint8Array>();
  freed = false;
  analyzeCalls: unknown[][] = [];
  spectrogramCalls: unknown[][] = [];
  meta: SessionMeta;
  run: () => Analysis;
  runSpectrogram: () => Spectrogram;
  constructor(meta: SessionMeta, run: () => Analysis, runSpectrogram: () => Spectrogram = spectrogram) {
    this.meta = meta;
    this.run = run;
    this.runSpectrogram = runSpectrogram;
  }
  add_file(name: string, bytes: Uint8Array) {
    this.files.set(name, bytes);
  }
  analyze(axis: unknown, params: unknown, overrides: unknown) {
    this.analyzeCalls.push([axis, params, overrides]);
    return this.run();
  }
  spectrogram(axis: unknown, chip: unknown, point: unknown, maxFreq: unknown) {
    this.spectrogramCalls.push([axis, chip, point, maxFreq]);
    return this.runSpectrogram();
  }
  free() {
    this.freed = true;
  }
}

function fakeModule(run: () => Analysis = analysis, runSpectrogram: () => Spectrogram = spectrogram) {
  const sessions: FakeSession[] = [];
  const module = {
    WasmSession: class extends FakeSession {
      constructor(meta: SessionMeta) {
        super(meta, run, runSpectrogram);
        sessions.push(this);
      }
    },
    config_snippet: (rec, axis) => `snippet ${rec.best.name} ${axis}`,
  } as WasmModule;
  return { module, sessions };
}

function spectrogramRequest(over: Partial<SpectrogramRequest> = {}): SpectrogramRequest {
  return {
    type: "spectrogram",
    id: 1,
    sessionId: "s1",
    meta: twoAxisMeta("s1"),
    axis: "X",
    chip: "a",
    point: 0,
    maxFreq: 200,
    files: {},
    ...over,
  };
}

function request(over: Partial<AnalyzeRequest> = {}): AnalyzeRequest {
  return {
    type: "analyze",
    id: 1,
    sessionId: "s1",
    meta: twoAxisMeta("s1"),
    axis: "X",
    params: defaultParams,
    overrides: [],
    files: {},
    ...over,
  };
}

const bytes = (n: number) => new Uint8Array([n]).buffer;

describe("the worker handler", () => {
  it("opens a new session when the same id comes back with another creation time", async () => {
    const { module, sessions } = fakeModule();
    const handle = createHandler(async () => module);
    const newer = { ...twoAxisMeta("s1"), created_at: "2026-11-01T00:00:00Z" };
    await handle(request({ files: { "c0-s0.zst": bytes(1) } }));
    await handle(request({ id: 2, meta: newer, files: { "c0-s0.zst": bytes(2) } }));
    expect(sessions).toHaveLength(2);
    expect(sessions[0].freed).toBe(true);
    expect(sessions[1].meta.created_at).toBe("2026-11-01T00:00:00Z");
    expect([...sessions[1].files.get("c0-s0.zst")!]).toEqual([2]);
  });

  it("passes the chips the user chose to include on to the analysis", async () => {
    const { module, sessions } = fakeModule();
    const handle = createHandler(async () => module);
    await handle(request({ overrides: ["b"] }));
    expect(sessions[0].analyzeCalls).toEqual([["X", defaultParams, ["b"]]]);
  });

  it("opens a session from its metadata, adds the files and returns the analysis with snippets", async () => {
    const { module, sessions } = fakeModule();
    const handle = createHandler(async () => module);
    const response = await handle(request({ files: { "c0-s0.zst": bytes(1), "c1-s0.zst": bytes(2) } }));
    expect(response).toMatchObject({ type: "result", id: 1 });
    expect([...sessions[0].files.keys()]).toEqual(["c0-s0.zst", "c1-s0.zst"]);
    expect(sessions[0].analyzeCalls).toEqual([["X", defaultParams, []]]);
    const result = (response as { result: { snippets: unknown } }).result;
    expect(result.snippets).toEqual({
      worstCase: "snippet mzv X",
      worstCasePerformance: null,
      perChip: [["snippet zv X", "snippet mzv X"], null],
    });
  });

  it("returns a config snippet for the performance recommendation when there is one", async () => {
    const withPerformance = () => {
      const a = analysis();
      a.worst_case = a.worst_case && { ...a.worst_case, for_performance: candidate("zv", 55) };
      return a;
    };
    const { module } = fakeModule(withPerformance);
    const handle = createHandler(async () => module);
    const response = await handle(request({ files: { "c0-s0.zst": bytes(1) } }));
    const result = (response as { result: { snippets: { worstCasePerformance: string | null } } }).result;
    expect(result.snippets.worstCasePerformance).toBe("snippet zv X");
  });

  it("answers a spectrogram request with the wasm result and the same id", async () => {
    const { module, sessions } = fakeModule();
    const handle = createHandler(async () => module);
    const response = await handle(spectrogramRequest({ id: 5, chip: "b", point: 1, maxFreq: 120 }));
    expect(response).toEqual({ type: "spectrogram", id: 5, result: spectrogram() });
    expect(sessions[0].spectrogramCalls).toEqual([["X", "b", 1, 120]]);
  });

  it("opens the session and adds the files for a spectrogram, and reuses it for an analysis", async () => {
    const { module, sessions } = fakeModule();
    const handle = createHandler(async () => module);
    await handle(spectrogramRequest({ files: { "c0-s0.zst": bytes(1) } }));
    await handle(request({ id: 2 }));
    expect(sessions).toHaveLength(1);
    expect([...sessions[0].files.keys()]).toEqual(["c0-s0.zst"]);
    expect(sessions[0].analyzeCalls).toHaveLength(1);
  });

  it("answers a spectrogram failure with its message and the same id", async () => {
    const { module } = fakeModule(analysis, () => {
      throw new Error("no recording of chip x at point 0 on this axis");
    });
    const handle = createHandler(async () => module);
    expect(await handle(spectrogramRequest({ id: 9 }))).toEqual({
      type: "error",
      id: 9,
      message: "no recording of chip x at point 0 on this axis",
    });
  });

  it("reuses the handle for a second request on the same session", async () => {
    const { module, sessions } = fakeModule();
    const handle = createHandler(async () => module);
    await handle(request({ files: { "c0-s0.zst": bytes(1) } }));
    await handle(request({ id: 2, axis: "Y" }));
    expect(sessions).toHaveLength(1);
    expect(sessions[0].analyzeCalls.map((c) => c[0])).toEqual(["X", "Y"]);
  });

  it("frees A's handle and starts a new one when B is requested", async () => {
    const { module, sessions } = fakeModule();
    const handle = createHandler(async () => module);
    await handle(request());
    await handle(request({ id: 2, sessionId: "b", meta: twoAxisMeta("b") }));
    expect(sessions).toHaveLength(2);
    expect(sessions[0].freed).toBe(true);
    expect(sessions[1].meta.id).toBe("b");
  });

  it("answers an analysis failure with its message and the same id", async () => {
    const { module } = fakeModule(() => {
      throw new Error("sample file c1-s0.zst is not loaded");
    });
    const handle = createHandler(async () => module);
    expect(await handle(request({ id: 7 }))).toEqual({
      type: "error",
      id: 7,
      message: "sample file c1-s0.zst is not loaded",
    });
  });

  it("answers every request with the load failure when the module cannot start", async () => {
    const handle = createHandler(() => Promise.reject(new Error("wasm fetch failed")));
    const first = await handle(request({ id: 1 }));
    const second = await handle(request({ id: 2 }));
    expect(first).toMatchObject({ type: "error", id: 1, message: "wasm fetch failed" });
    expect(second).toMatchObject({ type: "error", id: 2, message: "wasm fetch failed" });
  });

  it("builds a new handle from new metadata after forget on the same id", async () => {
    const { module, sessions } = fakeModule();
    const handle = createHandler(async () => module);
    await handle(request({ files: { "c0-s0.zst": bytes(1) } }));
    expect(await handle({ type: "forget", sessionId: "s1" })).toBeNull();
    const changed = twoAxisMeta("s1");
    changed.chips[0].name = "other chip";
    await handle(request({ id: 2, meta: changed, files: { "c0-s0.zst": bytes(9) } }));
    expect(sessions).toHaveLength(2);
    expect(sessions[0].freed).toBe(true);
    expect(sessions[1].files.get("c0-s0.zst")).toEqual(new Uint8Array([9]));
    expect(sessions[1].meta.chips[0].name).toBe("other chip");
  });
});
