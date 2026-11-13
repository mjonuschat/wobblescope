import type { AxesMapChip } from "@/lib/generated/AxesMapChip";
import type { Axis } from "@/lib/generated/Axis";
import type { FitParams } from "@/lib/generated/FitParams";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import { fetchResultSample as fetchResultSampleFile, fetchSample as fetchSampleFile } from "@/lib/api/endpoints";

import type { AnalysisResult, WorkerRequest, WorkerResponse } from "./protocol";

/** The part of a Web Worker the client uses, so tests can pass in a fake. */
export interface WorkerLike {
  postMessage(message: WorkerRequest, transfer: Transferable[]): void;
  terminate(): void;
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null;
  onerror: ((event: ErrorEvent) => void) | null;
}

export interface AnalysisJob {
  /** The cache key for the worker's files. Also the fetch route when `resultId` is not set. */
  sessionId: string;
  resultId?: string;
  meta: SessionMeta;
  axis: Axis;
  params: FitParams;
  /** Chips to include although their health or coverage would exclude them. */
  overrides: string[];
}

/** A queued request that a newer one replaced before it was sent. */
export class SupersededError extends Error {
  constructor() {
    super("A newer analysis request replaced this one");
    this.name = "SupersededError";
  }
}

/** A calibration session to analyse; its files are the session's own. */
export interface AxesMapJob {
  sessionId: string;
  meta: SessionMeta;
}

export interface AnalysisClient {
  analyze(job: AnalysisJob, signal?: AbortSignal): Promise<AnalysisResult>;
  /**
   * Analyses every chip of a calibration session. Each file is downloaded on its own: one that
   * fails is left out, and the chip that needs it comes back `Incomplete`.
   */
  analyzeAxesMap(job: AxesMapJob, signal?: AbortSignal): Promise<AxesMapChip[]>;
  /** Drops what is held for a session id that may be written again. */
  forget(sessionId: string): void;
}

interface Options {
  createWorker: () => WorkerLike;
  fetchSample?: typeof fetchSampleFile;
  fetchResultSample?: typeof fetchResultSampleFile;
}

type QueuedJob = ({ kind: "axis" } & AnalysisJob) | ({ kind: "axesMap" } & AxesMapJob);

interface Queued {
  job: QueuedJob;
  signal?: AbortSignal;
  resolve: (result: never) => void;
  reject: (error: unknown) => void;
}

function abortError(): DOMException {
  return new DOMException("The analysis was cancelled", "AbortError");
}

/** What the worker and the held-files logic key a job by, so a result never shares files with a session. */
function identityOf(job: QueuedJob): string {
  return job.kind === "axis" && job.resultId !== undefined ? `result:${job.resultId}` : job.sessionId;
}

/** The recordings of one axis, as (chip position, segment position, file name). */
function filesFor(meta: SessionMeta, axis: Axis) {
  return meta.segments.flatMap((segment, index) =>
    segment.axis === axis
      ? segment.files.map((file) => ({
          chip: file.chip,
          segment: index,
          name: file.sample_file,
        }))
      : [],
  );
}

/** Every recording of a calibration session, as (chip position, segment position, file name). */
function allFiles(meta: SessionMeta) {
  return meta.segments.flatMap((segment, index) =>
    segment.files.map((file) => ({ chip: file.chip, segment: index, name: file.sample_file })),
  );
}

/**
 * Owns the analysis worker. It fetches the sample files of the requested axis on the main
 * thread, sends the worker only the ones it lacks (as transferred buffers), and answers one
 * request at a time: a newer request replaces one still waiting.
 */
export function createAnalysisClient({
  createWorker,
  fetchSample = fetchSampleFile,
  fetchResultSample = fetchResultSampleFile,
}: Options): AnalysisClient {
  let worker: WorkerLike | null = null;
  let heldSession: string | null = null;
  let heldCreatedAt: string | null = null;
  let held = new Set<string>();
  let nextId = 1;
  const pending = new Map<number, { resolve: (r: never) => void; reject: (e: unknown) => void }>();
  let active: Queued | null = null;
  let waiting: Queued | null = null;

  function ensureWorker(): WorkerLike {
    if (worker) return worker;
    const created = createWorker();
    created.onmessage = (event) => {
      const response = event.data;
      if (response.type === "error") {
        // The worker may have stopped before storing this request's files, even when the
        // request was aborted and nobody waits for the answer.
        heldSession = null;
        held = new Set();
      }
      const entry = pending.get(response.id);
      if (!entry) return;
      pending.delete(response.id);
      if (response.type === "result") entry.resolve(response.result as never);
      else if (response.type === "axesMapResult") entry.resolve(response.chips as never);
      else entry.reject(new Error(response.message));
    };
    created.onerror = () => {
      const failure = new Error("The analysis module failed to load");
      for (const entry of pending.values()) entry.reject(failure);
      pending.clear();
      created.terminate();
      worker = null;
      heldSession = null;
      held = new Set();
    };
    worker = created;
    return created;
  }

  async function run({ job, signal }: Queued): Promise<AnalysisResult | AxesMapChip[]> {
    if (signal?.aborted) throw abortError();
    const target = ensureWorker();
    const identity = identityOf(job);
    const sameSession = heldSession === identity && heldCreatedAt === job.meta.created_at;
    const alreadyHeld = sameSession ? held : new Set<string>();
    const wanted = (job.kind === "axis" ? filesFor(job.meta, job.axis) : allFiles(job.meta)).filter(
      (f) => !alreadyHeld.has(f.name),
    );
    let missing = wanted;
    let buffers: ArrayBuffer[];
    if (job.kind === "axis") {
      buffers = await Promise.all(
        wanted.map((f) =>
          job.resultId !== undefined
            ? fetchResultSample(job.resultId, f.chip, f.segment, signal)
            : fetchSample(job.sessionId, f.chip, f.segment, signal),
        ),
      );
    } else {
      // A file that fails to download is left out so only its chip is reported incomplete.
      const settled = await Promise.allSettled(
        wanted.map((f) => fetchSample(job.sessionId, f.chip, f.segment, signal)),
      );
      missing = wanted.filter((_, i) => settled[i].status === "fulfilled");
      buffers = settled.flatMap((s) => (s.status === "fulfilled" ? [s.value] : []));
    }
    if (signal?.aborted) throw abortError();
    // The worker failed to load while the files were downloading, so nothing can answer.
    if (worker !== target) throw new Error("The analysis module failed to load");
    if (!sameSession) {
      heldSession = identity;
      heldCreatedAt = job.meta.created_at;
      held = new Set();
    }
    for (const f of missing) held.add(f.name);

    const id = nextId++;
    const files = Object.fromEntries(missing.map((f, i) => [f.name, buffers[i]]));
    return new Promise<AnalysisResult | AxesMapChip[]>((resolve, reject) => {
      pending.set(id, { resolve: resolve as (r: never) => void, reject });
      signal?.addEventListener(
        "abort",
        () => {
          pending.delete(id);
          // The files were posted, but the worker may fail before storing them.
          heldSession = null;
          held = new Set();
          reject(abortError());
        },
        { once: true },
      );
      target.postMessage(
        job.kind === "axis"
          ? {
              type: "analyze",
              id,
              sessionId: identity,
              meta: job.meta,
              axis: job.axis,
              params: job.params,
              overrides: job.overrides,
              files,
            }
          : { type: "analyzeAxesMap", id, sessionId: identity, meta: job.meta, files },
        buffers,
      );
    });
  }

  async function start(entry: Queued) {
    active = entry;
    try {
      entry.resolve((await run(entry)) as never);
    } catch (error) {
      entry.reject(error);
    } finally {
      active = null;
      const next = waiting;
      waiting = null;
      if (next) void start(next);
    }
  }

  function enqueue<T>(job: QueuedJob, signal?: AbortSignal): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      if (signal?.aborted) {
        reject(abortError());
        return;
      }
      const entry: Queued = { job, signal, resolve: resolve as (r: never) => void, reject };
      if (!active) {
        void start(entry);
        return;
      }
      waiting?.reject(new SupersededError());
      waiting = entry;
      signal?.addEventListener(
        "abort",
        () => {
          if (waiting === entry) {
            waiting = null;
            reject(abortError());
          }
        },
        { once: true },
      );
    });
  }

  return {
    analyze: (job, signal) => enqueue<AnalysisResult>({ kind: "axis", ...job }, signal),
    analyzeAxesMap: (job, signal) => enqueue<AxesMapChip[]>({ kind: "axesMap", ...job }, signal),
    forget(sessionId) {
      if (heldSession === sessionId) {
        heldSession = null;
        held = new Set();
      }
      worker?.postMessage({ type: "forget", sessionId }, []);
    },
  };
}

let shared: AnalysisClient | null = null;

/** The app's one client, which starts its worker on the first analysis. */
export function getAnalysisClient(): AnalysisClient {
  shared ??= createAnalysisClient({
    createWorker: () => new Worker(new URL("./worker.ts", import.meta.url), { type: "module" }),
  });
  return shared;
}
