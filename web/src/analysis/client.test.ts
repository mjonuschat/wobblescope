import { describe, expect, it, vi } from "vitest";

import { ApiClientError } from "@/lib/api/client";
import type { OnBytes } from "@/lib/api/endpoints";
import { analysis, defaultParams, spectrogram, twoAxisMeta } from "@/test/fixtures";

import { type AnalysisProgress, SupersededError, createAnalysisClient, type WorkerLike } from "./client";
import type { AnalyzeRequest, SpectrogramRequest, WorkerRequest, WorkerResponse } from "./protocol";

class FakeWorker implements WorkerLike {
  posted: { message: WorkerRequest; transfer: Transferable[] }[] = [];
  onmessage: WorkerLike["onmessage"] = null;
  onerror: WorkerLike["onerror"] = null;
  terminated = false;
  postMessage(message: WorkerRequest, transfer: Transferable[]) {
    this.posted.push({ message, transfer });
  }
  terminate() {
    this.terminated = true;
  }
  analyses() {
    return this.posted.map((p) => p.message).filter((m): m is AnalyzeRequest => m.type === "analyze");
  }
  spectrograms() {
    return this.posted.map((p) => p.message).filter((m): m is SpectrogramRequest => m.type === "spectrogram");
  }
  answerSpectrogram(id: number) {
    const response: WorkerResponse = { type: "spectrogram", id, result: spectrogram() };
    this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>);
  }
  answer(id: number) {
    const response: WorkerResponse = {
      type: "result",
      id,
      result: {
        analysis: analysis(),
        snippets: { worstCase: "s", worstCasePerformance: null, perChip: [["s"], null] },
      },
    };
    this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>);
  }
}

function setup() {
  const worker = new FakeWorker();
  const fetchSample = vi.fn(
    async (_id: string, chip: number, segment: number) => new Uint8Array([chip, segment]).buffer,
  );
  const createWorker = vi.fn(() => worker);
  const client = createAnalysisClient({ createWorker, fetchSample });
  const job = (over: Partial<Parameters<typeof client.analyze>[0]> = {}) => ({
    sessionId: "s1",
    meta: twoAxisMeta("s1"),
    axis: "X" as const,
    params: defaultParams,
    overrides: [] as string[],
    ...over,
  });
  const spectrogramJob = (over: Partial<Parameters<typeof client.spectrogram>[0]> = {}) => ({
    sessionId: "s1",
    meta: twoAxisMeta("s1"),
    axis: "X" as const,
    chip: "a",
    point: 0,
    maxFreq: 200,
    ...over,
  });
  return { worker, fetchSample, createWorker, client, job, spectrogramJob };
}

/** A controllable fetch that records onBytes calls and can be resolved or rejected later. */
interface Deferred {
  key: string;
  chip: number;
  segment: number;
  signal?: AbortSignal;
  onBytes?: OnBytes;
  resolve: (buffer: ArrayBuffer) => void;
  reject: (error: Error) => void;
  reportBytes: (loaded: number, total: number | null) => void;
}

function setupWithDeferredFetch() {
  const worker = new FakeWorker();
  const deferred: Deferred[] = [];
  const fetchSample = vi.fn(
    (id: string, chip: number, segment: number, signal?: AbortSignal, onBytes?: OnBytes): Promise<ArrayBuffer> => {
      return new Promise<ArrayBuffer>((resolve, reject) => {
        const entry: Deferred = {
          key: `${id}:${chip}/${segment}`,
          chip,
          segment,
          signal,
          onBytes,
          resolve,
          reject,
          reportBytes: (loaded, total) => onBytes?.(loaded, total),
        };
        deferred.push(entry);
      });
    },
  );
  const fetchResultSample = vi.fn(
    (id: string, chip: number, segment: number, signal?: AbortSignal, onBytes?: OnBytes): Promise<ArrayBuffer> => {
      return new Promise<ArrayBuffer>((resolve, reject) => {
        const entry: Deferred = {
          key: `result:${id}:${chip}/${segment}`,
          chip,
          segment,
          signal,
          onBytes,
          resolve,
          reject,
          reportBytes: (loaded, total) => onBytes?.(loaded, total),
        };
        deferred.push(entry);
      });
    },
  );
  const createWorker = vi.fn(() => worker);
  const client = createAnalysisClient({ createWorker, fetchSample, fetchResultSample });
  const job = (over: Partial<Parameters<typeof client.analyze>[0]> = {}) => ({
    sessionId: "s1",
    meta: twoAxisMeta("s1"),
    axis: "X" as const,
    params: defaultParams,
    overrides: [] as string[],
    ...over,
  });
  const prefetchJob = (over: Partial<Parameters<typeof client.prefetch>[0]> = {}) => ({
    sessionId: "s1",
    meta: twoAxisMeta("s1"),
    axis: "X" as const,
    ...over,
  });
  const spectrogramJob = (over: Partial<Parameters<typeof client.spectrogram>[0]> = {}) => ({
    sessionId: "s1",
    meta: twoAxisMeta("s1"),
    axis: "X" as const,
    chip: "a",
    point: 0,
    maxFreq: 200,
    ...over,
  });
  /** Finds a pending fetch by chip/segment. */
  const findFetch = (chip: number, segment: number) => deferred.find((d) => d.chip === chip && d.segment === segment);
  /** Resolves all pending fetches with a default buffer. */
  const resolveAllFetches = () => {
    for (const d of deferred) d.resolve(new Uint8Array([d.chip, d.segment]).buffer);
  };
  return {
    worker,
    fetchSample,
    fetchResultSample,
    createWorker,
    client,
    job,
    prefetchJob,
    spectrogramJob,
    deferred,
    findFetch,
    resolveAllFetches,
  };
}

/** Lets the client's pending fetches and posts run. */
const flush = () => new Promise((r) => setTimeout(r, 0));

describe("the analysis client", () => {
  it("fetches only the files an axis needs and never a file twice", async () => {
    const { worker, fetchSample, client, job } = setup();
    const a = client.analyze(job());
    await flush();
    worker.answer(worker.analyses()[0].id);
    await a;
    expect(fetchSample.mock.calls.map((c) => `${c[1]}/${c[2]}`)).toEqual(["0/0", "1/0"]);

    const b = client.analyze(job({ axis: "Y" }));
    await flush();
    worker.answer(worker.analyses()[1].id);
    await b;
    expect(fetchSample.mock.calls.map((c) => `${c[1]}/${c[2]}`)).toEqual(["0/0", "1/0", "0/1"]);

    const c = client.analyze(job());
    await flush();
    worker.answer(worker.analyses()[2].id);
    await c;
    expect(fetchSample).toHaveBeenCalledTimes(3);
    expect(Object.keys(worker.analyses()[2].files)).toEqual([]);
  });

  it("sends the chips to include along with the request", async () => {
    const { worker, client, job } = setup();
    void client.analyze(job({ overrides: ["b"] })).catch(() => {});
    await flush();
    expect(worker.analyses()[0].overrides).toEqual(["b"]);
  });

  it("transfers the fetched buffers to the worker", async () => {
    const { worker, client, job } = setup();
    void client.analyze(job()).catch(() => {});
    await flush();
    const { message, transfer } = worker.posted[0];
    const files = (message as AnalyzeRequest).files;
    expect(Object.keys(files)).toEqual(["c0-s0.zst", "c1-s0.zst"]);
    expect(transfer).toEqual(Object.values(files));
  });

  it("refetches the files when another session is requested", async () => {
    const { worker, fetchSample, client, job } = setup();
    const a = client.analyze(job());
    await flush();
    worker.answer(worker.analyses()[0].id);
    await a;
    const b = client.analyze(job({ sessionId: "s2", meta: twoAxisMeta("s2") }));
    await flush();
    worker.answer(worker.analyses()[1].id);
    await b;
    expect(fetchSample).toHaveBeenCalledTimes(4);
  });

  it("fetches a result job its own samples from the result route even when a session shares its id and creation time", async () => {
    const { worker, fetchSample, job } = setup();
    const fetchResultSample = vi.fn(
      async (_id: string, chip: number, segment: number) => new Uint8Array([9, chip, segment]).buffer,
    );
    const shared = createAnalysisClient({
      createWorker: () => worker,
      fetchSample,
      fetchResultSample,
    });
    const a = shared.analyze(job());
    await flush();
    worker.answer(worker.analyses()[0].id);
    await a;
    const b = shared.analyze(job({ resultId: "r1" }));
    await flush();
    worker.answer(worker.analyses()[1].id);
    await b;
    expect(fetchSample).toHaveBeenCalledTimes(2);
    expect(fetchResultSample.mock.calls.map((c) => `${c[0]}:${c[1]}/${c[2]}`)).toEqual(["r1:0/0", "r1:1/0"]);
    expect(worker.analyses()[1].sessionId).toBe("result:r1");

    const c = shared.analyze(job({ resultId: "r2" }));
    await flush();
    worker.answer(worker.analyses()[2].id);
    await c;
    expect(fetchResultSample).toHaveBeenCalledTimes(4);
  });

  it("fetches every file again and tells the worker after the session is forgotten", async () => {
    const { worker, fetchSample, client, job } = setup();
    const a = client.analyze(job());
    await flush();
    worker.answer(worker.analyses()[0].id);
    await a;
    client.forget("s1");
    expect(worker.posted.at(-1)?.message).toEqual({ type: "forget", sessionId: "s1" });
    const b = client.analyze(job());
    await flush();
    worker.answer(worker.analyses()[1].id);
    await b;
    expect(fetchSample).toHaveBeenCalledTimes(4);
  });

  it("fetches the axis files for a spectrogram once and reuses the held session", async () => {
    const { worker, fetchSample, client, job, spectrogramJob } = setup();
    const first = client.analyze(job());
    await flush();
    worker.answer(worker.analyses()[0].id);
    await first;
    const fetched = fetchSample.mock.calls.length;
    const map = client.spectrogram(spectrogramJob({ chip: "b", point: 1, maxFreq: 120 }));
    await flush();
    const [request] = worker.spectrograms();
    expect(request).toMatchObject({ axis: "X", chip: "b", point: 1, maxFreq: 120, sessionId: "s1", files: {} });
    expect(fetchSample).toHaveBeenCalledTimes(fetched);
    worker.answerSpectrogram(request.id);
    expect(await map).toEqual(spectrogram());
  });

  it("keeps a waiting analysis when a spectrogram is requested, and runs the analysis first", async () => {
    const { worker, client, job, spectrogramJob } = setup();
    const first = client.analyze(job());
    const waiting = client.analyze(job({ axis: "Y" }));
    const map = client.spectrogram(spectrogramJob());
    await flush();
    worker.answer(worker.analyses()[0].id);
    await first;
    await flush();
    expect(worker.analyses().map((m) => m.axis)).toEqual(["X", "Y"]);
    expect(worker.spectrograms()).toHaveLength(0);
    worker.answer(worker.analyses()[1].id);
    await waiting;
    await flush();
    worker.answerSpectrogram(worker.spectrograms()[0].id);
    await map;
  });

  it("replaces a waiting spectrogram with a newer one, never posting it", async () => {
    const { worker, client, job, spectrogramJob } = setup();
    const first = client.analyze(job());
    const older = client.spectrogram(spectrogramJob({ chip: "a" }));
    const newer = client.spectrogram(spectrogramJob({ chip: "b" }));
    await expect(older).rejects.toBeInstanceOf(SupersededError);
    await flush();
    worker.answer(worker.analyses()[0].id);
    await first;
    await flush();
    worker.answerSpectrogram(worker.spectrograms()[0].id);
    await newer;
    expect(worker.spectrograms().map((m) => m.chip)).toEqual(["b"]);
  });

  it("rejects an aborted spectrogram request and ignores its late result", async () => {
    const { worker, client, spectrogramJob } = setup();
    const controller = new AbortController();
    const map = client.spectrogram(spectrogramJob(), controller.signal);
    await flush();
    const id = worker.spectrograms()[0].id;
    controller.abort();
    await expect(map).rejects.toMatchObject({ name: "AbortError" });
    expect(() => worker.answerSpectrogram(id)).not.toThrow();
  });

  it("drops a queued request that a newer one replaces, never posting it", async () => {
    const { worker, client, job } = setup();
    const first = client.analyze(job());
    const second = client.analyze(job({ axis: "Y" }));
    const third = client.analyze(job({ axis: "X", params: { ...defaultParams, scv: 6 } }));
    await expect(second).rejects.toBeInstanceOf(SupersededError);
    await flush();
    worker.answer(worker.analyses()[0].id);
    await first;
    await flush();
    worker.answer(worker.analyses()[1].id);
    await third;
    expect(worker.analyses().map((m) => m.axis)).toEqual(["X", "X"]);
    expect(worker.analyses()[1].params.scv).toBe(6);
  });

  it("rejects an aborted request and ignores its late result", async () => {
    const { worker, client, job } = setup();
    const controller = new AbortController();
    const result = client.analyze(job(), controller.signal);
    await flush();
    const id = worker.analyses()[0].id;
    controller.abort();
    await expect(result).rejects.toMatchObject({ name: "AbortError" });
    expect(() => worker.answer(id)).not.toThrow();
  });

  it("rejects with the API error and posts nothing when a sample fetch fails", async () => {
    const { worker, fetchSample, client, job } = setup();
    fetchSample.mockRejectedValueOnce(new ApiClientError("gone", 404, { error: "not_found", message: "gone" }));
    await expect(client.analyze(job())).rejects.toMatchObject({ status: 404 });
    expect(worker.posted).toHaveLength(0);
  });

  it("rejects the pending request when the worker fails to load", async () => {
    const { worker, client, job } = setup();
    const result = client.analyze(job());
    await flush();
    worker.onerror?.({} as ErrorEvent);
    await expect(result).rejects.toThrow("analysis module failed to load");
    expect(worker.terminated).toBe(true);
  });

  it("rejects an analysis error from the worker with its message", async () => {
    const { worker, client, job } = setup();
    const result = client.analyze(job());
    await flush();
    worker.onmessage?.({
      data: { type: "error", id: worker.analyses()[0].id, message: "bad data" },
    } as MessageEvent<WorkerResponse>);
    await expect(result).rejects.toThrow("bad data");
  });

  it("sends the files again for the same id with another creation time", async () => {
    const { worker, fetchSample, client, job } = setup();
    const first = client.analyze(job());
    await flush();
    worker.answer(worker.analyses()[0].id);
    await first;

    const newer = { ...twoAxisMeta("s1"), created_at: "2026-11-01T00:00:00Z" };
    const second = client.analyze(job({ meta: newer }));
    await flush();
    worker.answer(worker.analyses()[1].id);
    await second;
    expect(fetchSample).toHaveBeenCalledTimes(4);
    expect(Object.keys(worker.analyses()[1].files).length).toBe(2);
  });

  it("refetches the files after the worker answers with an error", async () => {
    const { worker, fetchSample, client, job } = setup();
    const first = client.analyze(job());
    await flush();
    worker.onmessage?.({
      data: { type: "error", id: worker.analyses()[0].id, message: "bad meta" },
    } as MessageEvent<WorkerResponse>);
    await expect(first).rejects.toThrow("bad meta");

    void client.analyze(job()).catch(() => {});
    await flush();
    expect(fetchSample).toHaveBeenCalledTimes(4);
    expect(Object.keys(worker.analyses()[1].files).length).toBe(2);
  });

  it("refetches the files when the worker errors after the request was aborted", async () => {
    const { worker, fetchSample, client, job } = setup();
    const controller = new AbortController();
    const first = client.analyze(job(), controller.signal);
    await flush();
    controller.abort();
    await expect(first).rejects.toThrow();
    worker.onmessage?.({
      data: { type: "error", id: worker.analyses()[0].id, message: "bad meta" },
    } as MessageEvent<WorkerResponse>);

    void client.analyze(job()).catch(() => {});
    await flush();
    expect(fetchSample).toHaveBeenCalledTimes(4);
    expect(Object.keys(worker.analyses()[1].files).length).toBe(2);
  });

  it("refetches the files for a retry that starts before the aborted request answers", async () => {
    const { worker, fetchSample, client, job } = setup();
    const controller = new AbortController();
    const first = client.analyze(job(), controller.signal);
    await flush();
    controller.abort();
    await expect(first).rejects.toThrow();

    void client.analyze(job()).catch(() => {});
    await flush();
    expect(fetchSample).toHaveBeenCalledTimes(4);
    expect(Object.keys(worker.analyses()[1].files).length).toBe(2);
  });

  it("settles when the worker fails to load during the downloads, and serves the next request", async () => {
    const { worker, fetchSample, createWorker, client, job } = setup();
    let finish: (buffer: ArrayBuffer) => void = () => {};
    fetchSample.mockImplementationOnce(() => new Promise<ArrayBuffer>((r) => (finish = r)));
    const result = client.analyze(job({ axis: "Y" }));
    const failed = expect(result).rejects.toThrow("analysis module failed to load");
    await flush();
    worker.onerror?.({} as ErrorEvent);
    finish(new ArrayBuffer(1));
    await failed;
    expect(worker.analyses()).toHaveLength(0);

    // The queue moves on: the next request starts a new worker and is answered.
    const next = client.analyze(job());
    await flush();
    expect(createWorker).toHaveBeenCalledTimes(2);
    worker.answer(worker.analyses()[0].id);
    await expect(next).resolves.toBeTruthy();
  });

  it("the client fetches a result job\u2019s samples from the result route", async () => {
    const worker = new FakeWorker();
    const fetchSample = vi.fn(async () => new Uint8Array([1]).buffer);
    const fetchResultSample = vi.fn(async () => new Uint8Array([2]).buffer);
    const client = createAnalysisClient({ createWorker: () => worker, fetchSample, fetchResultSample });
    const pending = client.analyze({
      sessionId: "s1",
      resultId: "r1",
      meta: twoAxisMeta("s1"),
      axis: "X",
      params: defaultParams,
      overrides: [],
    });
    await flush();
    expect(fetchSample).not.toHaveBeenCalled();
    expect(fetchResultSample.mock.calls.map((c) => c.slice(0, 3))).toEqual([
      ["r1", 0, 0],
      ["r1", 1, 0],
    ]);
    worker.answer(1);
    await pending;
  });
});

describe("analysis progress", () => {
  it("reports downloading, then analysing", async () => {
    const { worker, client, job, findFetch, deferred } = setupWithDeferredFetch();
    const progress: AnalysisProgress[] = [];
    const p = client.analyze(job(), undefined, (prog) => progress.push(prog));
    await flush();
    // Both fetches are pending, but neither has reported total yet
    expect(progress).toEqual([{ phase: "downloading", loaded: 0, total: null }]);

    // First file reports its progress
    const f1 = findFetch(0, 0)!;
    f1.reportBytes(500, 1000);
    expect(progress.at(-1)).toEqual({ phase: "downloading", loaded: 500, total: null });

    // Second file reports its total
    const f2 = findFetch(1, 0)!;
    f2.reportBytes(200, 500);
    // Now total is the sum since both have reported
    expect(progress.at(-1)).toEqual({ phase: "downloading", loaded: 700, total: 1500 });

    // Both finish
    f1.resolve(new Uint8Array([0, 0]).buffer);
    f2.resolve(new Uint8Array([1, 0]).buffer);
    await flush();

    // Now analysing
    expect(progress.at(-1)).toEqual({ phase: "analysing" });
    expect(worker.analyses()).toHaveLength(1);

    worker.answer(worker.analyses()[0].id);
    await p;
    expect(deferred).toHaveLength(2);
  });

  it("reports queued for a request behind the active one", async () => {
    const { worker, client, job, findFetch } = setupWithDeferredFetch();
    const firstProgress: AnalysisProgress[] = [];
    const first = client.analyze(job(), undefined, (p) => firstProgress.push(p));
    await flush();

    // Second request while first is downloading
    const secondProgress: AnalysisProgress[] = [];
    const second = client.analyze(job({ axis: "Y" }), undefined, (p) => secondProgress.push(p));
    await flush();

    // Second reports queued
    expect(secondProgress).toEqual([{ phase: "queued" }]);

    // Finish first
    findFetch(0, 0)!.resolve(new Uint8Array([0, 0]).buffer);
    findFetch(1, 0)!.resolve(new Uint8Array([1, 0]).buffer);
    await flush();
    worker.answer(worker.analyses()[0].id);
    await first;

    // Second now starts and reports downloading
    await flush();
    expect(secondProgress.at(-1)).toEqual({ phase: "downloading", loaded: 0, total: null });

    findFetch(0, 1)!.resolve(new Uint8Array([0, 1]).buffer);
    await flush();
    worker.answer(worker.analyses()[1].id);
    await second;
  });

  it("reports analysing at once when the worker holds every file", async () => {
    const { worker, client, job, findFetch } = setupWithDeferredFetch();
    // First analysis to fill the worker cache
    const first = client.analyze(job());
    await flush();
    findFetch(0, 0)!.resolve(new Uint8Array([0, 0]).buffer);
    findFetch(1, 0)!.resolve(new Uint8Array([1, 0]).buffer);
    await flush();
    worker.answer(worker.analyses()[0].id);
    await first;

    // Second with same files - worker already has them
    const progress: AnalysisProgress[] = [];
    const second = client.analyze(job({ params: { ...defaultParams, scv: 6 } }), undefined, (p) => progress.push(p));
    await flush();

    // No downloading phase, straight to analysing
    expect(progress).toEqual([{ phase: "analysing" }]);
    worker.answer(worker.analyses()[1].id);
    await second;
  });

  it("prefetch downloads the other axis outside the request slot", async () => {
    const { worker, client, job, prefetchJob, findFetch, fetchSample } = setupWithDeferredFetch();
    // X analysis
    const xResult = client.analyze(job());
    await flush();
    findFetch(0, 0)!.resolve(new Uint8Array([0, 0]).buffer);
    findFetch(1, 0)!.resolve(new Uint8Array([1, 0]).buffer);
    await flush();
    worker.answer(worker.analyses()[0].id);
    await xResult;

    // Prefetch Y
    client.prefetch(prefetchJob({ axis: "Y" }));
    await flush();

    // Y has one file (0/1) - it should be fetching now
    expect(fetchSample.mock.calls.filter((c) => c[2] === 1).length).toBe(1);

    // An X request with different params should run immediately without waiting
    const progress: AnalysisProgress[] = [];
    const xAgain = client.analyze(job({ params: { ...defaultParams, scv: 7 } }), undefined, (p) => progress.push(p));
    await flush();

    // X files are held, so analysing right away
    expect(progress).toEqual([{ phase: "analysing" }]);
    worker.answer(worker.analyses()[1].id);
    await xAgain;
  });

  it("a request reuses a background download and counts its bytes", async () => {
    const { worker, client, job, prefetchJob, findFetch, fetchSample } = setupWithDeferredFetch();
    // X analysis to fill held
    const xResult = client.analyze(job());
    await flush();
    findFetch(0, 0)!.resolve(new Uint8Array([0, 0]).buffer);
    findFetch(1, 0)!.resolve(new Uint8Array([1, 0]).buffer);
    await flush();
    worker.answer(worker.analyses()[0].id);
    await xResult;

    // Prefetch Y
    client.prefetch(prefetchJob({ axis: "Y" }));
    await flush();
    const yFetch = findFetch(0, 1)!;

    // Request Y while prefetch is still downloading
    const progress: AnalysisProgress[] = [];
    const yResult = client.analyze(job({ axis: "Y" }), undefined, (p) => progress.push(p));
    await flush();

    // Should be downloading (reusing prefetch)
    expect(progress.at(-1)?.phase).toBe("downloading");

    // Report progress on the prefetch - should show in request progress
    yFetch.reportBytes(300, 1000);
    expect(progress.at(-1)).toEqual({ phase: "downloading", loaded: 300, total: 1000 });

    // Complete the prefetch
    yFetch.resolve(new Uint8Array([0, 1]).buffer);
    await flush();

    expect(progress.at(-1)).toEqual({ phase: "analysing" });
    worker.answer(worker.analyses()[1].id);
    await yResult;

    // fetchSample was called only once for Y file
    expect(fetchSample.mock.calls.filter((c) => c[2] === 1).length).toBe(1);
  });

  it("prefetch skips files already held or already downloading", async () => {
    const { worker, client, job, prefetchJob, findFetch, fetchSample } = setupWithDeferredFetch();
    // X has two files. After analysis, worker holds them.
    const first = client.analyze(job());
    await flush();
    findFetch(0, 0)!.resolve(new Uint8Array([0, 0]).buffer);
    findFetch(1, 0)!.resolve(new Uint8Array([1, 0]).buffer);
    await flush();
    worker.answer(worker.analyses()[0].id);
    await first;

    const callsBefore = fetchSample.mock.calls.length;

    // Prefetch X - already held, no new fetches
    client.prefetch(prefetchJob({ axis: "X" }));
    await flush();
    expect(fetchSample.mock.calls.length).toBe(callsBefore);

    // Prefetch Y starts a fetch
    client.prefetch(prefetchJob({ axis: "Y" }));
    await flush();
    const yFetchesA = fetchSample.mock.calls.filter((c) => c[2] === 1).length;
    expect(yFetchesA).toBe(1);

    // Prefetch Y again - should not start another fetch
    client.prefetch(prefetchJob({ axis: "Y" }));
    await flush();
    const yFetchesB = fetchSample.mock.calls.filter((c) => c[2] === 1).length;
    expect(yFetchesB).toBe(1);
  });

  it("a failed background download is fetched again by the request", async () => {
    const { worker, client, job, prefetchJob, findFetch, fetchSample, deferred } = setupWithDeferredFetch();
    // X analysis
    const first = client.analyze(job());
    await flush();
    findFetch(0, 0)!.resolve(new Uint8Array([0, 0]).buffer);
    findFetch(1, 0)!.resolve(new Uint8Array([1, 0]).buffer);
    await flush();
    worker.answer(worker.analyses()[0].id);
    await first;

    // Prefetch Y and fail it
    client.prefetch(prefetchJob({ axis: "Y" }));
    await flush();
    const yPrefetch = deferred.find((d) => d.segment === 1)!;
    yPrefetch.reject(new Error("network error"));
    await flush();

    // Request Y - should fetch again
    const yResult = client.analyze(job({ axis: "Y" }));
    await flush();

    // A new fetch was started
    const yFetches = fetchSample.mock.calls.filter((c) => c[2] === 1);
    expect(yFetches.length).toBe(2);

    // Complete it
    const newYFetch = deferred.find((d) => d.segment === 1 && d !== yPrefetch)!;
    newYFetch.resolve(new Uint8Array([0, 1]).buffer);
    await flush();
    worker.answer(worker.analyses()[1].id);
    await yResult;
  });

  it("opening another session aborts its background downloads", async () => {
    const { worker, client, job, prefetchJob, findFetch, deferred, fetchSample } = setupWithDeferredFetch();
    // s1 X analysis
    const first = client.analyze(job());
    await flush();
    findFetch(0, 0)!.resolve(new Uint8Array([0, 0]).buffer);
    findFetch(1, 0)!.resolve(new Uint8Array([1, 0]).buffer);
    await flush();
    worker.answer(worker.analyses()[0].id);
    await first;

    // Prefetch s1 Y
    client.prefetch(prefetchJob({ axis: "Y" }));
    await flush();
    const s1YFetch = deferred.find((d) => d.key === "s1:0/1")!;
    expect(s1YFetch).toBeDefined();
    expect(s1YFetch.signal?.aborted).toBe(false);

    // Analyze s2 - should abort s1 prefetch
    const s2 = client.analyze(job({ sessionId: "s2", meta: twoAxisMeta("s2") }));
    await flush();

    expect(s1YFetch.signal?.aborted).toBe(true);

    // s2 should start its own fetches
    const s2Fetches = fetchSample.mock.calls.filter((c) => c[0] === "s2");
    expect(s2Fetches.length).toBe(2);

    // Complete s2
    for (const d of deferred.filter((d) => d.key.startsWith("s2:"))) {
      d.resolve(new Uint8Array([d.chip, d.segment]).buffer);
    }
    await flush();
    worker.answer(worker.analyses()[1].id);
    await s2;
  });

  it("forget drops the background downloads of that session", async () => {
    const { worker, client, job, prefetchJob, findFetch, deferred, fetchSample } = setupWithDeferredFetch();
    // s1 X analysis
    const first = client.analyze(job());
    await flush();
    findFetch(0, 0)!.resolve(new Uint8Array([0, 0]).buffer);
    findFetch(1, 0)!.resolve(new Uint8Array([1, 0]).buffer);
    await flush();
    worker.answer(worker.analyses()[0].id);
    await first;

    // Prefetch Y
    client.prefetch(prefetchJob({ axis: "Y" }));
    await flush();
    const yPrefetch = deferred.find((d) => d.segment === 1)!;
    expect(yPrefetch.signal?.aborted).toBe(false);

    // Forget s1
    client.forget("s1");
    expect(yPrefetch.signal?.aborted).toBe(true);

    // A later analyze(Y) fetches again
    const yResult = client.analyze(job({ axis: "Y" }));
    await flush();
    const yFetches = fetchSample.mock.calls.filter((c) => c[2] === 1);
    expect(yFetches.length).toBe(2);

    for (const d of deferred.filter((d) => d.segment === 1 && d !== yPrefetch)) {
      d.resolve(new Uint8Array([d.chip, d.segment]).buffer);
    }
    await flush();
    worker.answer(worker.analyses()[1].id);
    await yResult;
  });

  it("an aborted request leaves the background download for the next one", async () => {
    const { worker, client, job, prefetchJob, findFetch, deferred, fetchSample } = setupWithDeferredFetch();
    // X analysis to fill held
    const first = client.analyze(job());
    await flush();
    findFetch(0, 0)!.resolve(new Uint8Array([0, 0]).buffer);
    findFetch(1, 0)!.resolve(new Uint8Array([1, 0]).buffer);
    await flush();
    worker.answer(worker.analyses()[0].id);
    await first;

    // Prefetch Y
    client.prefetch(prefetchJob({ axis: "Y" }));
    await flush();

    // Request Y with abort while waiting on download
    const controller = new AbortController();
    const yFirst = client.analyze(job({ axis: "Y" }), controller.signal);
    await flush();
    controller.abort();
    await expect(yFirst).rejects.toMatchObject({ name: "AbortError" });

    // The background download is NOT aborted
    const yPrefetch = deferred.find((d) => d.segment === 1)!;
    expect(yPrefetch.signal?.aborted).toBe(false);

    // Second Y request uses the same background download
    const ySecond = client.analyze(job({ axis: "Y" }));
    await flush();

    // No new fetch was started
    expect(fetchSample.mock.calls.filter((c) => c[2] === 1).length).toBe(1);

    // Complete and verify
    yPrefetch.resolve(new Uint8Array([0, 1]).buffer);
    await flush();
    worker.answer(worker.analyses()[1].id);
    await ySecond;
  });

  it("an aborted request waiting on a background download frees the slot at once", async () => {
    const { worker, client, job, prefetchJob, findFetch, deferred } = setupWithDeferredFetch();
    // X analysis to fill held
    const first = client.analyze(job());
    await flush();
    findFetch(0, 0)!.resolve(new Uint8Array([0, 0]).buffer);
    findFetch(1, 0)!.resolve(new Uint8Array([1, 0]).buffer);
    await flush();
    worker.answer(worker.analyses()[0].id);
    await first;

    // Prefetch Y with hanging download
    client.prefetch(prefetchJob({ axis: "Y" }));
    await flush();

    // Analyze Y, waiting on the prefetch
    const controller = new AbortController();
    const yReq = client.analyze(job({ axis: "Y" }), controller.signal);
    await flush();

    // Abort it
    controller.abort();
    await expect(yReq).rejects.toMatchObject({ name: "AbortError" });

    // Start X with different params - should run immediately (slot is free)
    const progress: AnalysisProgress[] = [];
    const xReq = client.analyze(job({ params: { ...defaultParams, scv: 8 } }), undefined, (p) => progress.push(p));
    await flush();

    // X is analysing (not queued), slot was freed immediately
    expect(progress).toEqual([{ phase: "analysing" }]);

    // The Y downloads are NOT aborted (shared background download)
    const yPrefetch = deferred.find((d) => d.segment === 1)!;
    expect(yPrefetch.signal?.aborted).toBe(false);

    worker.answer(worker.analyses()[1].id);
    await xReq;
  });
});
