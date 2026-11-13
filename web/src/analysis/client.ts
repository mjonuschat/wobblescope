import type { Axis } from "@/lib/generated/Axis";
import type { FitParams } from "@/lib/generated/FitParams";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import type { Spectrogram } from "@/lib/generated/Spectrogram";
import {
  type OnBytes,
  fetchResultSample as fetchResultSampleFile,
  fetchSample as fetchSampleFile,
} from "@/lib/api/endpoints";

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

export interface SpectrogramJob {
  /** The cache key for the worker's files. Also the fetch route when `resultId` is not set. */
  sessionId: string;
  resultId?: string;
  meta: SessionMeta;
  axis: Axis;
  chip: string;
  point: number;
  /** The map keeps the frequencies up to this, in Hz. */
  maxFreq: number;
}

/** A queued request that a newer one replaced before it was sent. */
export class SupersededError extends Error {
  constructor() {
    super("A newer analysis request replaced this one");
    this.name = "SupersededError";
  }
}

export type AnalysisProgress =
  | { phase: "queued" }
  | { phase: "downloading"; loaded: number; total: number | null }
  | { phase: "analysing" };

export interface PrefetchJob {
  sessionId: string;
  resultId?: string;
  meta: SessionMeta;
  axis: Axis;
}

export interface AnalysisClient {
  analyze(job: AnalysisJob, signal?: AbortSignal, onProgress?: (p: AnalysisProgress) => void): Promise<AnalysisResult>;
  /** The time-frequency map of one chip's recording. It runs after the analysis in the queue. */
  spectrogram(job: SpectrogramJob, signal?: AbortSignal): Promise<Spectrogram>;
  /** Starts downloading the axis's files outside the request slot. */
  prefetch(job: PrefetchJob): void;
  /** Drops what is held for a session id that may be written again. */
  forget(sessionId: string): void;
}

interface Options {
  createWorker: () => WorkerLike;
  fetchSample?: typeof fetchSampleFile;
  fetchResultSample?: typeof fetchResultSampleFile;
}

type Kind = "analysis" | "spectrogram";

type Queued =
  | {
      kind: "analysis";
      job: AnalysisJob;
      signal?: AbortSignal;
      onProgress?: (p: AnalysisProgress) => void;
      resolve: (result: AnalysisResult) => void;
      reject: (error: unknown) => void;
    }
  | {
      kind: "spectrogram";
      job: SpectrogramJob;
      signal?: AbortSignal;
      resolve: (result: Spectrogram) => void;
      reject: (error: unknown) => void;
    };

function abortError(): DOMException {
  return new DOMException("The analysis was cancelled", "AbortError");
}

interface JobWithIdentity {
  sessionId: string;
  resultId?: string;
}

/** What the worker and the held-files logic key a job by, so a result never shares files with a session. */
function identityOf(job: JobWithIdentity): string {
  return job.resultId !== undefined ? `result:${job.resultId}` : job.sessionId;
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

/** A pending background download with progress tracking. */
interface Download {
  promise: Promise<ArrayBuffer>;
  loaded: number;
  total: number | null;
  /** Progress listeners that `run` subscribes to. */
  listeners: Set<OnBytes>;
}

/**
 * Owns the analysis worker. It fetches the sample files of the requested axis on the main
 * thread, sends the worker only the ones it lacks (as transferred buffers), and answers one
 * request at a time. A newer request replaces a waiting one of the same kind, and a waiting
 * analysis goes before a waiting spectrogram.
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
  const pending = new Map<
    number,
    { resolve: (r: AnalysisResult | Spectrogram) => void; reject: (e: unknown) => void }
  >();
  let active: Queued | null = null;
  const waiting: Record<Kind, Queued | null> = { analysis: null, spectrogram: null };

  // Background download state, scoped to one identity and created_at.
  let bgIdentity: string | null = null;
  let bgCreatedAt: string | null = null;
  let bgController: AbortController | null = null;
  const bgDownloads = new Map<string, Download>();

  function clearBackgroundDownloads() {
    bgController?.abort();
    bgController = null;
    bgIdentity = null;
    bgCreatedAt = null;
    bgDownloads.clear();
  }

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
      if (response.type === "error") entry.reject(new Error(response.message));
      else entry.resolve(response.result);
    };
    created.onerror = () => {
      const failure = new Error("The analysis module failed to load");
      for (const entry of pending.values()) entry.reject(failure);
      pending.clear();
      created.terminate();
      worker = null;
      heldSession = null;
      held = new Set();
      clearBackgroundDownloads();
    };
    worker = created;
    return created;
  }

  async function run(queued: Queued): Promise<AnalysisResult | Spectrogram> {
    const { job, signal } = queued;
    const onProgress = queued.kind === "analysis" ? queued.onProgress : undefined;
    if (signal?.aborted) throw abortError();
    const target = ensureWorker();
    const identity = identityOf(job);
    const sameSession = heldSession === identity && heldCreatedAt === job.meta.created_at;
    const alreadyHeld = sameSession ? held : new Set<string>();
    const missing = filesFor(job.meta, job.axis).filter((f) => !alreadyHeld.has(f.name));

    // If nothing is missing, report analysing right away.
    if (missing.length === 0) {
      onProgress?.({ phase: "analysing" });
    } else {
      onProgress?.({ phase: "downloading", loaded: 0, total: null });
    }

    // Check if background downloads match this job's scope.
    const bgMatchesScope = bgIdentity === identity && bgCreatedAt === job.meta.created_at;

    // Per-file progress tracking for the sum reported to onProgress.
    const fileProgress = new Map<string, { loaded: number; total: number | null }>();
    for (const f of missing) {
      fileProgress.set(f.name, { loaded: 0, total: null });
    }

    function reportDownloadProgress() {
      let loaded = 0;
      let total: number | null = 0;
      for (const fp of fileProgress.values()) {
        loaded += fp.loaded;
        if (total !== null) {
          if (fp.total !== null) {
            total += fp.total;
          } else {
            total = null;
          }
        }
      }
      onProgress?.({ phase: "downloading", loaded, total });
    }

    // Fetch files, reusing background downloads when the scope matches.
    const buffers = await Promise.all(
      missing.map(async (f) => {
        const fileOnBytes: OnBytes = (loaded, total) => {
          fileProgress.set(f.name, { loaded, total });
          reportDownloadProgress();
        };

        // Try to reuse a background download.
        if (bgMatchesScope) {
          const bg = bgDownloads.get(f.name);
          if (bg) {
            // Subscribe to updates from the background download.
            bg.listeners.add(fileOnBytes);
            // Initialize with current progress.
            fileProgress.set(f.name, { loaded: bg.loaded, total: bg.total });
            try {
              // Race between background download and request abort.
              const buffer = await Promise.race([
                bg.promise,
                new Promise<never>((_, reject) => {
                  signal?.addEventListener("abort", () => reject(abortError()), { once: true });
                }),
              ]);
              bg.listeners.delete(fileOnBytes);
              return buffer;
            } catch {
              bg.listeners.delete(fileOnBytes);
              // If the request was aborted, rethrow.
              if (signal?.aborted) throw abortError();
              // Otherwise the background download failed; fall through to fetch directly.
            }
          }
        }

        // Fetch directly.
        return job.resultId !== undefined
          ? fetchResultSample(job.resultId, f.chip, f.segment, signal, fileOnBytes)
          : fetchSample(job.sessionId, f.chip, f.segment, signal, fileOnBytes);
      }),
    );

    if (signal?.aborted) throw abortError();
    // The worker failed to load while the files were downloading, so nothing can answer.
    if (worker !== target) throw new Error("The analysis module failed to load");
    if (!sameSession) {
      heldSession = identity;
      heldCreatedAt = job.meta.created_at;
      held = new Set();
    }
    for (const f of missing) {
      held.add(f.name);
      // Remove from background downloads after posting to worker.
      if (bgMatchesScope) {
        bgDownloads.delete(f.name);
      }
    }

    // Report analysing only if we had files to download (otherwise already reported above).
    if (missing.length > 0) {
      onProgress?.({ phase: "analysing" });
    }

    const id = nextId++;
    const files = Object.fromEntries(missing.map((f, i) => [f.name, buffers[i]]));
    return new Promise<AnalysisResult | Spectrogram>((resolve, reject) => {
      pending.set(id, { resolve, reject });
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
        queued.kind === "analysis"
          ? {
              type: "analyze",
              id,
              sessionId: identity,
              meta: job.meta,
              axis: job.axis,
              params: queued.job.params,
              overrides: queued.job.overrides,
              files,
            }
          : {
              type: "spectrogram",
              id,
              sessionId: identity,
              meta: job.meta,
              axis: job.axis,
              chip: queued.job.chip,
              point: queued.job.point,
              maxFreq: queued.job.maxFreq,
              files,
            },
        buffers,
      );
    });
  }

  async function start(entry: Queued) {
    active = entry;
    try {
      (entry.resolve as (result: unknown) => void)(await run(entry));
    } catch (error) {
      entry.reject(error);
    } finally {
      active = null;
      const kind: Kind | null = waiting.analysis ? "analysis" : waiting.spectrogram ? "spectrogram" : null;
      const next = kind && waiting[kind];
      if (kind) waiting[kind] = null;
      if (next) void start(next);
    }
  }

  /** Starts `entry` now, or queues it behind the active request in the slot of its kind. */
  function submit(entry: Queued) {
    const { kind, signal } = entry;
    if (!active) {
      void start(entry);
      return;
    }
    waiting[kind]?.reject(new SupersededError());
    waiting[kind] = entry;
    if (kind === "analysis") {
      (entry as Extract<Queued, { kind: "analysis" }>).onProgress?.({ phase: "queued" });
    }
    signal?.addEventListener(
      "abort",
      () => {
        if (waiting[kind] === entry) {
          waiting[kind] = null;
          entry.reject(abortError());
        }
      },
      { once: true },
    );
  }

  function startPrefetch(job: PrefetchJob) {
    const identity = identityOf(job);

    // If scope changed, clear old background downloads.
    if (bgIdentity !== identity || bgCreatedAt !== job.meta.created_at) {
      clearBackgroundDownloads();
      bgIdentity = identity;
      bgCreatedAt = job.meta.created_at;
      bgController = new AbortController();
    }

    // Determine which files to skip (held by worker or already downloading).
    const sameSession = heldSession === identity && heldCreatedAt === job.meta.created_at;
    const alreadyHeld = sameSession ? held : new Set<string>();
    const filesToFetch = filesFor(job.meta, job.axis).filter(
      (f) => !alreadyHeld.has(f.name) && !bgDownloads.has(f.name),
    );

    // bgController is always defined here because we just created it above if it was missing.
    const signal = bgController?.signal;
    for (const f of filesToFetch) {
      const download: Download = {
        promise: null as unknown as Promise<ArrayBuffer>,
        loaded: 0,
        total: null,
        listeners: new Set(),
      };

      const onBytes: OnBytes = (loaded, total) => {
        download.loaded = loaded;
        download.total = total;
        for (const listener of download.listeners) {
          listener(loaded, total);
        }
      };

      const promise = (async () => {
        return job.resultId !== undefined
          ? await fetchResultSample(job.resultId, f.chip, f.segment, signal, onBytes)
          : await fetchSample(job.sessionId, f.chip, f.segment, signal, onBytes);
      })();
      // Handle errors: remove from map so a request fetches it again, but do not rethrow.
      promise.catch(() => {
        bgDownloads.delete(f.name);
      });
      download.promise = promise;
      bgDownloads.set(f.name, download);
    }
  }

  return {
    analyze(job, signal, onProgress) {
      return new Promise<AnalysisResult>((resolve, reject) => {
        if (signal?.aborted) {
          reject(abortError());
          return;
        }
        const identity = identityOf(job);
        // If the job's identity or created_at differs from the background scope, clear it.
        if (bgIdentity !== null && (bgIdentity !== identity || bgCreatedAt !== job.meta.created_at)) {
          clearBackgroundDownloads();
        }
        submit({ kind: "analysis", job, signal, onProgress, resolve, reject });
      });
    },
    spectrogram(job, signal) {
      return new Promise<Spectrogram>((resolve, reject) => {
        if (signal?.aborted) {
          reject(abortError());
          return;
        }
        submit({ kind: "spectrogram", job, signal, resolve, reject });
      });
    },
    prefetch(job) {
      startPrefetch(job);
    },
    forget(sessionId) {
      if (heldSession === sessionId) {
        heldSession = null;
        held = new Set();
      }
      // Also clear background downloads for this session.
      if (bgIdentity === sessionId) {
        clearBackgroundDownloads();
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
