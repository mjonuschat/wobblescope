import { QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import { createQueryClient } from "@/lib/api/queries";
import { twoAxisMeta, vibrationsAnalysis } from "@/test/fixtures";

import { createAnalysisClient, type WorkerLike } from "./client";
import type { WorkerRequest, WorkerResponse } from "./protocol";
import { useVibrations } from "./useVibrations";

function setup() {
  const requests: WorkerRequest[] = [];
  const worker: WorkerLike = {
    onmessage: null,
    onerror: null,
    terminate() {},
    postMessage(message) {
      requests.push(message);
      if (message.type !== "vibrations") return;
      const response: WorkerResponse = { type: "vibrations", id: message.id, result: vibrationsAnalysis() };
      queueMicrotask(() => this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>));
    },
  };
  const fetchSample = vi.fn(async () => new ArrayBuffer(1));
  const client = createAnalysisClient({ createWorker: () => worker, fetchSample });
  const queryClient = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
  const meta = twoAxisMeta();
  const hook = (over: Partial<Parameters<typeof useVibrations>[0]> = {}, enabled = true) =>
    renderHook(
      (props: { enabled: boolean }) =>
        useVibrations({
          sessionId: "s1",
          meta,
          maxFreq: 1000,
          includeExcluded: false,
          enabled: props.enabled,
          client,
          ...over,
        }),
      { wrapper, initialProps: { enabled } },
    );
  const vibrations = () => requests.filter((r) => r.type === "vibrations");
  return { hook, vibrations, fetchSample };
}

describe("useVibrations", () => {
  it("runs_once_per_key_and_keeps_its_result", async () => {
    const { hook, vibrations, fetchSample } = setup();
    for (let i = 0; i < 2; i++) {
      const { result } = hook({ maxFreq: 500, includeExcluded: true });
      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data).toEqual(vibrationsAnalysis());
    }
    expect(vibrations()).toMatchObject([{ maxFreq: 500, includeExcluded: true }]);
    // Every recording of the session is fetched, once: three files in the fixture.
    expect(fetchSample).toHaveBeenCalledTimes(3);
  });

  it("waits_until_enabled_and_meta_is_there", async () => {
    const { hook, vibrations, fetchSample } = setup();
    const withoutMeta = hook({ meta: undefined });
    expect(withoutMeta.result.current.fetchStatus).toBe("idle");
    const { result, rerender } = hook({}, false);
    expect(vibrations()).toHaveLength(0);
    expect(fetchSample).not.toHaveBeenCalled();
    rerender({ enabled: true });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(vibrations()).toHaveLength(1);
  });

  it("a_new_max_freq_makes_a_new_query", async () => {
    const { hook, vibrations } = setup();
    for (const maxFreq of [1000, 1000, 500]) {
      const { result } = hook({ maxFreq });
      await waitFor(() => expect(result.current.isSuccess).toBe(true));
    }
    expect(vibrations()).toHaveLength(2);
  });
});
