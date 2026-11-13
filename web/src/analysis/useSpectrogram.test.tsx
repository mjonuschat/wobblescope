import { QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import { createQueryClient } from "@/lib/api/queries";
import { spectrogram, twoAxisMeta } from "@/test/fixtures";

import { createAnalysisClient, type WorkerLike } from "./client";
import type { WorkerRequest, WorkerResponse } from "./protocol";
import { useSpectrogram } from "./useSpectrogram";

function setup() {
  const requests: WorkerRequest[] = [];
  const worker: WorkerLike = {
    onmessage: null,
    onerror: null,
    terminate() {},
    postMessage(message) {
      requests.push(message);
      if (message.type !== "spectrogram") return;
      const response: WorkerResponse = { type: "spectrogram", id: message.id, result: spectrogram() };
      queueMicrotask(() => this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>));
    },
  };
  const fetchSample = vi.fn(async () => new ArrayBuffer(1));
  const fetchResultSample = vi.fn(async () => new ArrayBuffer(1));
  const client = createAnalysisClient({ createWorker: () => worker, fetchSample, fetchResultSample });
  const queryClient = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
  const meta = twoAxisMeta();
  const hook = (over: Partial<Parameters<typeof useSpectrogram>[0]> = {}, enabled = true) =>
    renderHook(
      (props: { enabled: boolean }) =>
        useSpectrogram({
          sessionId: "s1",
          meta,
          axis: "X",
          chip: "a",
          point: 0,
          maxFreq: 200,
          enabled: props.enabled,
          client,
          ...over,
        }),
      { wrapper, initialProps: { enabled } },
    );
  const spectrograms = () => requests.filter((r) => r.type === "spectrogram").length;
  return { hook, spectrograms, fetchSample, fetchResultSample };
}

describe("useSpectrogram", () => {
  it("does nothing until it is enabled", async () => {
    const { hook, spectrograms, fetchSample } = setup();
    const { result, rerender } = hook({}, false);
    expect(spectrograms()).toBe(0);
    expect(fetchSample).not.toHaveBeenCalled();
    rerender({ enabled: true });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(spectrogram());
  });

  it("does not run without a chip, a point or the metadata", () => {
    const { hook, spectrograms } = setup();
    hook({ chip: null });
    hook({ point: null });
    hook({ meta: undefined });
    expect(spectrograms()).toBe(0);
  });

  it("fetches once and serves the same key from the cache", async () => {
    const { hook, spectrograms } = setup();
    const first = hook();
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    expect(spectrograms()).toBe(1);
    first.rerender({ enabled: true });
    expect(first.result.current.data).toEqual(spectrogram());
    expect(spectrograms()).toBe(1);
  });

  it("asks again for another chip, point or maximum frequency", async () => {
    const { hook, spectrograms } = setup();
    for (const over of [{ chip: "b" }, { point: 1 }, { maxFreq: 120 }]) {
      const { result } = hook(over);
      await waitFor(() => expect(result.current.isSuccess).toBe(true));
    }
    expect(spectrograms()).toBe(3);
  });

  it("keys a saved result apart from a session with the same id and fetches from the result route", async () => {
    const { hook, spectrograms, fetchSample, fetchResultSample } = setup();
    const session = hook();
    await waitFor(() => expect(session.result.current.isSuccess).toBe(true));
    const saved = hook({ sessionId: "result:r1", resultId: "r1" });
    await waitFor(() => expect(saved.result.current.isSuccess).toBe(true));
    expect(spectrograms()).toBe(2);
    expect(fetchSample).toHaveBeenCalled();
    expect(fetchResultSample).toHaveBeenCalled();
  });
});
