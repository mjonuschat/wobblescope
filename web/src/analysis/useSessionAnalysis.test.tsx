import { QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import { createQueryClient, queryKeys } from "@/lib/api/queries";
import { analysis, defaultParams, twoAxisMeta } from "@/test/fixtures";

import { createAnalysisClient, type WorkerLike } from "./client";
import type { WorkerRequest, WorkerResponse } from "./protocol";
import { useSessionAnalysis } from "./useSessionAnalysis";

/** A worker that answers every analysis request at once. */
function setup() {
  const requests: WorkerRequest[] = [];
  const worker: WorkerLike = {
    onmessage: null,
    onerror: null,
    terminate() {},
    postMessage(message) {
      requests.push(message);
      if (message.type !== "analyze") return;
      const response: WorkerResponse = {
        type: "result",
        id: message.id,
        result: {
          analysis: analysis(),
          snippets: { worstCase: "s", worstCasePerformance: null, perChip: [["s"], null] },
        },
      };
      queueMicrotask(() => this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>));
    },
  };
  const fetchSample = vi.fn(async () => new ArrayBuffer(1));
  const client = createAnalysisClient({ createWorker: () => worker, fetchSample });
  client.prefetch = vi.fn();
  const queryClient = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
  const hook = (params = defaultParams, overrides: string[] = [], createdAt?: string) => {
    const meta = { ...twoAxisMeta(), ...(createdAt ? { created_at: createdAt } : {}) };
    return renderHook(
      ({ p, o }) => useSessionAnalysis({ sessionId: "s1", meta, axis: "X", params: p, overrides: o, client }),
      { wrapper, initialProps: { p: params, o: overrides } },
    );
  };
  const analyses = () => requests.filter((r) => r.type === "analyze").length;
  return { hook, analyses, fetchSample, requests, queryClient };
}

describe("useSessionAnalysis", () => {
  it("never shows the previous parameters\u2019 result for new ones", async () => {
    const { hook } = setup();
    const { result, rerender } = hook();
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    rerender({ p: { ...defaultParams, scv: 7 }, o: [] });
    expect(result.current.data).toBeUndefined();
    expect(result.current.isPending).toBe(true);
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
  });

  it("serves a new mount from the cache without analysing or fetching again", async () => {
    const { hook, analyses, fetchSample } = setup();
    const first = hook();
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    first.unmount();
    const second = hook();
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));
    expect(analyses()).toBe(1);
    expect(fetchSample).toHaveBeenCalledTimes(2); // The first mount's two X files only.
  });

  it("serves earlier parameters from the cache when they come back", async () => {
    const { hook, analyses } = setup();
    const { result, rerender } = hook();
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    rerender({ p: { ...defaultParams, scv: 7 }, o: [] });
    await waitFor(() => expect(analyses()).toBe(2));
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    rerender({ p: defaultParams, o: [] });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(analyses()).toBe(2);
  });

  it("analyses again once the session\u2019s results were dropped", async () => {
    const { hook, analyses, queryClient } = setup();
    const first = hook();
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    first.unmount();
    queryClient.removeQueries({ queryKey: queryKeys.analysesOf("s1") });
    const second = hook();
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));
    expect(analyses()).toBe(2);
  });

  it("analyses again for a session written again under the same id", async () => {
    const { hook, analyses } = setup();
    const first = hook();
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    first.unmount();
    const second = hook(defaultParams, [], "2026-11-01T00:00:00Z");
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));
    expect(analyses()).toBe(2);
  });

  it("analyses again, with the chips named, when the overrides change", async () => {
    const { hook, analyses, requests } = setup();
    const { result, rerender } = hook();
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    rerender({ p: defaultParams, o: ["b"] });
    expect(result.current.data).toBeUndefined();
    await waitFor(() => expect(analyses()).toBe(2));
    const last = requests.filter((r) => r.type === "analyze").at(-1);
    expect(last).toMatchObject({ overrides: ["b"] });
  });

  it("starts as queued and follows the client\u2019s reports", async () => {
    let capturedOnProgress: ((p: import("./client").AnalysisProgress) => void) | undefined;
    const worker: WorkerLike = {
      onmessage: null,
      onerror: null,
      terminate() {},
      postMessage() {},
    };
    const client = createAnalysisClient({
      createWorker: () => worker,
      fetchSample: vi.fn(async () => new ArrayBuffer(1)),
    });
    client.analyze = vi.fn((_job, _signal, onProgress) => {
      capturedOnProgress = onProgress;
      return new Promise<import("./protocol").AnalysisResult>(() => {});
    });
    const queryClient = createQueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(
      () => useSessionAnalysis({ sessionId: "s1", meta: twoAxisMeta(), axis: "X", params: defaultParams, client }),
      { wrapper },
    );
    expect(result.current.progress).toEqual({ phase: "queued" });
    await waitFor(() => expect(capturedOnProgress).toBeDefined());
    capturedOnProgress!({ phase: "downloading", loaded: 10, total: 100 });
    await waitFor(() => expect(result.current.progress).toEqual({ phase: "downloading", loaded: 10, total: 100 }));
    capturedOnProgress!({ phase: "analysing" });
    await waitFor(() => expect(result.current.progress).toEqual({ phase: "analysing" }));
  });

  it("does not show progress of the previous parameters", async () => {
    let capturedOnProgress: ((p: import("./client").AnalysisProgress) => void) | undefined;
    const worker: WorkerLike = {
      onmessage: null,
      onerror: null,
      terminate() {},
      postMessage() {},
    };
    const client = createAnalysisClient({
      createWorker: () => worker,
      fetchSample: vi.fn(async () => new ArrayBuffer(1)),
    });
    client.analyze = vi.fn((_job, _signal, onProgress) => {
      capturedOnProgress = onProgress;
      return new Promise<import("./protocol").AnalysisResult>(() => {});
    });
    const queryClient = createQueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const { result, rerender } = renderHook(
      ({ p }) => useSessionAnalysis({ sessionId: "s1", meta: twoAxisMeta(), axis: "X", params: p, client }),
      { wrapper, initialProps: { p: defaultParams } },
    );
    await waitFor(() => expect(capturedOnProgress).toBeDefined());
    const firstOnProgress = capturedOnProgress!;
    firstOnProgress({ phase: "downloading", loaded: 10, total: 100 });
    await waitFor(() => expect(result.current.progress).toEqual({ phase: "downloading", loaded: 10, total: 100 }));
    rerender({ p: { ...defaultParams, scv: 7 } });
    expect(result.current.progress).toEqual({ phase: "queued" });
    await waitFor(() => expect(capturedOnProgress).not.toBe(firstOnProgress));
    firstOnProgress({ phase: "analysing" });
    await waitFor(() => expect(result.current.progress).toEqual({ phase: "queued" }));
  });

  it("a late report of the previous parameters does not erase the current one", async () => {
    let capturedOnProgress: ((p: import("./client").AnalysisProgress) => void) | undefined;
    const worker: WorkerLike = {
      onmessage: null,
      onerror: null,
      terminate() {},
      postMessage() {},
    };
    const client = createAnalysisClient({
      createWorker: () => worker,
      fetchSample: vi.fn(async () => new ArrayBuffer(1)),
    });
    client.analyze = vi.fn((_job, _signal, onProgress) => {
      capturedOnProgress = onProgress;
      return new Promise<import("./protocol").AnalysisResult>(() => {});
    });
    const queryClient = createQueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const { result, rerender } = renderHook(
      ({ p }) => useSessionAnalysis({ sessionId: "s1", meta: twoAxisMeta(), axis: "X", params: p, client }),
      { wrapper, initialProps: { p: defaultParams } },
    );
    await waitFor(() => expect(capturedOnProgress).toBeDefined());
    const firstOnProgress = capturedOnProgress!;
    rerender({ p: { ...defaultParams, scv: 7 } });
    await waitFor(() => expect(capturedOnProgress).not.toBe(firstOnProgress));
    const secondOnProgress = capturedOnProgress!;
    secondOnProgress({ phase: "downloading", loaded: 5, total: 50 });
    await waitFor(() => expect(result.current.progress).toEqual({ phase: "downloading", loaded: 5, total: 50 }));
    firstOnProgress({ phase: "analysing" });
    await waitFor(() => expect(result.current.progress).toEqual({ phase: "downloading", loaded: 5, total: 50 }));
  });

  it("never prefetches for a saved result", async () => {
    const worker: WorkerLike = {
      onmessage: null,
      onerror: null,
      terminate() {},
      postMessage(message) {
        if (message.type !== "analyze") return;
        const response: WorkerResponse = {
          type: "result",
          id: message.id,
          result: {
            analysis: analysis(),
            snippets: { worstCase: "s", worstCasePerformance: null, perChip: [["s"], null] },
          },
        };
        queueMicrotask(() => this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>));
      },
    };
    const client = createAnalysisClient({
      createWorker: () => worker,
      fetchSample: vi.fn(async () => new ArrayBuffer(1)),
      fetchResultSample: vi.fn(async () => new ArrayBuffer(1)),
    });
    const prefetch = vi.fn();
    client.prefetch = prefetch;
    const queryClient = createQueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(
      () =>
        useSessionAnalysis({
          sessionId: "s1",
          resultId: "r1",
          meta: twoAxisMeta(),
          axis: "X",
          params: defaultParams,
          client,
        }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(prefetch).not.toHaveBeenCalled();
  });

  it("downloads the other axes once the shown axis is analysed", async () => {
    const prefetch = vi.fn();
    const worker: WorkerLike = {
      onmessage: null,
      onerror: null,
      terminate() {},
      postMessage(message) {
        if (message.type !== "analyze") return;
        const response: WorkerResponse = {
          type: "result",
          id: message.id,
          result: {
            analysis: analysis(),
            snippets: { worstCase: "s", worstCasePerformance: null, perChip: [["s"], null] },
          },
        };
        queueMicrotask(() => this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>));
      },
    };
    const client = createAnalysisClient({
      createWorker: () => worker,
      fetchSample: vi.fn(async () => new ArrayBuffer(1)),
    });
    client.prefetch = prefetch;
    const queryClient = createQueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const meta = twoAxisMeta();
    const { result } = renderHook(
      () => useSessionAnalysis({ sessionId: "s1", meta, axis: "X", params: defaultParams, client }),
      { wrapper },
    );
    expect(prefetch).not.toHaveBeenCalled();
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(prefetch).toHaveBeenCalledTimes(1);
    expect(prefetch).toHaveBeenCalledWith({ sessionId: "s1", resultId: undefined, meta, axis: "Y" });
  });

  it("prefetches each axis only once even when fit params change", async () => {
    const prefetch = vi.fn();
    const worker: WorkerLike = {
      onmessage: null,
      onerror: null,
      terminate() {},
      postMessage(message) {
        if (message.type !== "analyze") return;
        const response: WorkerResponse = {
          type: "result",
          id: message.id,
          result: {
            analysis: analysis(),
            snippets: { worstCase: "s", worstCasePerformance: null, perChip: [["s"], null] },
          },
        };
        queueMicrotask(() => this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>));
      },
    };
    const client = createAnalysisClient({
      createWorker: () => worker,
      fetchSample: vi.fn(async () => new ArrayBuffer(1)),
    });
    client.prefetch = prefetch;
    const queryClient = createQueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const meta = twoAxisMeta();
    const { result, rerender } = renderHook(
      ({ p }) => useSessionAnalysis({ sessionId: "s1", meta, axis: "X", params: p, client }),
      { wrapper, initialProps: { p: defaultParams } },
    );
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(prefetch).toHaveBeenCalledTimes(1);
    // Change fit params - this triggers a new analysis
    rerender({ p: { ...defaultParams, scv: 7 } });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(prefetch).toHaveBeenCalledTimes(1);
  });

  it("prefetches the other axis again when created_at changes", async () => {
    const prefetch = vi.fn();
    const worker: WorkerLike = {
      onmessage: null,
      onerror: null,
      terminate() {},
      postMessage(message) {
        if (message.type !== "analyze") return;
        const response: WorkerResponse = {
          type: "result",
          id: message.id,
          result: {
            analysis: analysis(),
            snippets: { worstCase: "s", worstCasePerformance: null, perChip: [["s"], null] },
          },
        };
        queueMicrotask(() => this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>));
      },
    };
    const client = createAnalysisClient({
      createWorker: () => worker,
      fetchSample: vi.fn(async () => new ArrayBuffer(1)),
    });
    client.prefetch = prefetch;
    const queryClient = createQueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const meta = twoAxisMeta();
    const { result, rerender } = renderHook(
      ({ m }) => useSessionAnalysis({ sessionId: "s1", meta: m, axis: "X", params: defaultParams, client }),
      { wrapper, initialProps: { m: meta } },
    );
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(prefetch).toHaveBeenCalledTimes(1);
    expect(prefetch).toHaveBeenCalledWith({ sessionId: "s1", resultId: undefined, meta, axis: "Y" });
    const newMeta = { ...meta, created_at: "2026-11-02T00:00:00Z" };
    rerender({ m: newMeta });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(prefetch).toHaveBeenCalledTimes(2);
    expect(prefetch).toHaveBeenCalledWith({ sessionId: "s1", resultId: undefined, meta: newMeta, axis: "Y" });
  });

  it("prefetches each axis only once even when meta object identity changes", async () => {
    const prefetch = vi.fn();
    const worker: WorkerLike = {
      onmessage: null,
      onerror: null,
      terminate() {},
      postMessage(message) {
        if (message.type !== "analyze") return;
        const response: WorkerResponse = {
          type: "result",
          id: message.id,
          result: {
            analysis: analysis(),
            snippets: { worstCase: "s", worstCasePerformance: null, perChip: [["s"], null] },
          },
        };
        queueMicrotask(() => this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>));
      },
    };
    const client = createAnalysisClient({
      createWorker: () => worker,
      fetchSample: vi.fn(async () => new ArrayBuffer(1)),
    });
    client.prefetch = prefetch;
    const queryClient = createQueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const meta = twoAxisMeta();
    const { result, rerender } = renderHook(
      ({ m }) => useSessionAnalysis({ sessionId: "s1", meta: m, axis: "X", params: defaultParams, client }),
      { wrapper, initialProps: { m: meta } },
    );
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(prefetch).toHaveBeenCalledTimes(1);
    // Parent re-renders with new meta object (same content, same created_at)
    rerender({ m: { ...meta } });
    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(prefetch).toHaveBeenCalledTimes(1);
  });

  it("prefetches again when returning to a session after another session", async () => {
    const prefetch = vi.fn();
    const worker: WorkerLike = {
      onmessage: null,
      onerror: null,
      terminate() {},
      postMessage(message) {
        if (message.type !== "analyze") return;
        const response: WorkerResponse = {
          type: "result",
          id: message.id,
          result: {
            analysis: analysis(),
            snippets: { worstCase: "s", worstCasePerformance: null, perChip: [["s"], null] },
          },
        };
        queueMicrotask(() => this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>));
      },
    };
    const client = createAnalysisClient({
      createWorker: () => worker,
      fetchSample: vi.fn(async () => new ArrayBuffer(1)),
    });
    client.prefetch = prefetch;
    const queryClient = createQueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const meta = twoAxisMeta();
    const { result, rerender } = renderHook(
      ({ sid, m }) => useSessionAnalysis({ sessionId: sid, meta: m, axis: "X", params: defaultParams, client }),
      { wrapper, initialProps: { sid: "s1", m: meta } },
    );
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(prefetch).toHaveBeenCalledTimes(1);
    expect(prefetch).toHaveBeenCalledWith({ sessionId: "s1", resultId: undefined, meta, axis: "Y" });
    rerender({ sid: "s2", m: meta });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(prefetch).toHaveBeenCalledTimes(2);
    expect(prefetch).toHaveBeenCalledWith({ sessionId: "s2", resultId: undefined, meta, axis: "Y" });
    rerender({ sid: "s1", m: meta });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(prefetch).toHaveBeenCalledTimes(3);
    expect(prefetch).toHaveBeenCalledWith({ sessionId: "s1", resultId: undefined, meta, axis: "Y" });
  });
});
