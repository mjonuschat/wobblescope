import { describe, expect, it, vi } from "vitest";

import { ApiClientError } from "@/lib/api/client";
import { analysis, defaultParams, twoAxisMeta } from "@/test/fixtures";

import { SupersededError, createAnalysisClient, type WorkerLike } from "./client";
import type { AnalyzeRequest, WorkerRequest, WorkerResponse } from "./protocol";

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
  answer(id: number) {
    const response: WorkerResponse = {
      type: "result",
      id,
      result: { analysis: analysis(), snippets: { worstCase: "s", perChip: ["s", null] } },
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
  return { worker, fetchSample, createWorker, client, job };
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
});
