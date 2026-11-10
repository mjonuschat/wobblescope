import type { Analysis } from "@/lib/generated/Analysis";
import type { Axis } from "@/lib/generated/Axis";
import type { FitParams } from "@/lib/generated/FitParams";
import type { Recommendation } from "@/lib/generated/Recommendation";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import type { AnalysisResult, WorkerRequest, WorkerResponse } from "./protocol";

/** One session open inside wasm. */
export interface SessionHandle {
  add_file(name: string, bytes: Uint8Array): void;
  analyze(axis: Axis, params: FitParams, overrides: string[]): Analysis;
  free(): void;
}

/** The part of the wasm module the worker uses. */
export interface WasmModule {
  WasmSession: new (meta: SessionMeta) => SessionHandle;
  config_snippet(recommendation: Recommendation, axis: Axis): string;
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Builds the worker's message handler around a module loader. The worker holds one session at
 * a time: a request for another session replaces it, and `forget` frees it. A module that
 * fails to load answers every request with that failure.
 */
export function createHandler(loadModule: () => Promise<WasmModule>) {
  let loading: Promise<WasmModule> | null = null;
  let current: { sessionId: string; createdAt: string; handle: SessionHandle } | null = null;

  function release() {
    current?.handle.free();
    current = null;
  }

  async function analyze(request: Extract<WorkerRequest, { type: "analyze" }>) {
    loading ??= loadModule();
    const module = await loading;
    // A session written again under the same id has a new creation time and other files.
    if (current?.sessionId !== request.sessionId || current.createdAt !== request.meta.created_at) {
      release();
      current = {
        sessionId: request.sessionId,
        createdAt: request.meta.created_at,
        handle: new module.WasmSession(request.meta),
      };
    }
    for (const [name, buffer] of Object.entries(request.files)) {
      current!.handle.add_file(name, new Uint8Array(buffer));
    }
    const analysis = current!.handle.analyze(request.axis, request.params, request.overrides);
    const snippet = (r: Recommendation | null) => (r ? module.config_snippet(r, request.axis) : null);
    const result: AnalysisResult = {
      analysis,
      snippets: {
        worstCase: snippet(analysis.worst_case),
        perChip: analysis.per_chip.map((chip) => snippet(chip.recommendation)),
      },
    };
    return result;
  }

  return async function handle(request: WorkerRequest): Promise<WorkerResponse | null> {
    if (request.type === "forget") {
      if (current?.sessionId === request.sessionId) release();
      return null;
    }
    try {
      return { type: "result", id: request.id, result: await analyze(request) };
    } catch (error) {
      return { type: "error", id: request.id, message: describe(error) };
    }
  };
}
