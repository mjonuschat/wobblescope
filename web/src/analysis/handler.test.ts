import { describe, expect, it } from "vitest";

import { analysis, defaultParams, twoAxisMeta } from "@/test/fixtures";
import type { Analysis } from "@/lib/generated/Analysis";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import { createHandler, type SessionHandle, type WasmModule } from "./handler";
import type { AnalyzeRequest } from "./protocol";

class FakeSession implements SessionHandle {
  files = new Map<string, Uint8Array>();
  freed = false;
  analyzeCalls: unknown[][] = [];
  meta: SessionMeta;
  run: () => Analysis;
  constructor(meta: SessionMeta, run: () => Analysis) {
    this.meta = meta;
    this.run = run;
  }
  add_file(name: string, bytes: Uint8Array) {
    this.files.set(name, bytes);
  }
  analyze(axis: unknown, params: unknown, overrides: unknown) {
    this.analyzeCalls.push([axis, params, overrides]);
    return this.run();
  }
  free() {
    this.freed = true;
  }
}

function fakeModule(run: () => Analysis = analysis) {
  const sessions: FakeSession[] = [];
  const module = {
    WasmSession: class extends FakeSession {
      constructor(meta: SessionMeta) {
        super(meta, run);
        sessions.push(this);
      }
    },
    config_snippet: (rec, axis) => `snippet ${rec.best.name} ${axis}`,
  } as WasmModule;
  return { module, sessions };
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
      perChip: [["snippet zv X", "snippet mzv X"], null],
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
