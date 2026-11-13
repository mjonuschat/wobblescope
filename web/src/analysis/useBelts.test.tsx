import { QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import { createQueryClient } from "@/lib/api/queries";
import { beltsAnalysis, twoAxisMeta } from "@/test/fixtures";

import { createAnalysisClient, type WorkerLike } from "./client";
import type { WorkerRequest, WorkerResponse } from "./protocol";
import { useBelts } from "./useBelts";

function setup() {
  const requests: WorkerRequest[] = [];
  const worker: WorkerLike = {
    onmessage: null,
    onerror: null,
    terminate() {},
    postMessage(message) {
      requests.push(message);
      if (message.type !== "belts") return;
      const response: WorkerResponse = { type: "belts", id: message.id, result: beltsAnalysis(message.chip) };
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
  const hook = (over: Partial<Parameters<typeof useBelts>[0]> = {}, enabled = true) =>
    renderHook(
      (props: { enabled: boolean }) =>
        useBelts({
          sessionId: "s1",
          meta,
          chip: "adxl345",
          maxFreq: 200,
          includeExcluded: false,
          enabled: props.enabled,
          client,
          ...over,
        }),
      { wrapper, initialProps: { enabled } },
    );
  const belts = () => requests.filter((r) => r.type === "belts");
  return { hook, belts, fetchSample };
}

describe("useBelts", () => {
  it("it_asks_for_the_chosen_chip_and_max_freq", async () => {
    const { hook, belts, fetchSample } = setup();
    const { result } = hook({ chip: "lis2dw ebb36", maxFreq: 120, includeExcluded: true });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(beltsAnalysis("lis2dw ebb36"));
    expect(belts()).toMatchObject([{ chip: "lis2dw ebb36", maxFreq: 120, includeExcluded: true }]);
    // Only that chip's recording (chip 1, X segment 0) is fetched.
    expect(fetchSample).toHaveBeenCalledTimes(1);
    expect(fetchSample).toHaveBeenCalledWith("s1", 1, 0, expect.anything());
  });

  it("it_waits_until_enabled_and_meta_is_there", async () => {
    const { hook, belts, fetchSample } = setup();
    hook({ meta: undefined });
    const { result, rerender } = hook({}, false);
    expect(belts()).toHaveLength(0);
    expect(fetchSample).not.toHaveBeenCalled();
    rerender({ enabled: true });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(belts()).toHaveLength(1);
  });

  it("a_new_max_freq_makes_a_new_query", async () => {
    const { hook, belts } = setup();
    for (const maxFreq of [200, 200, 120]) {
      const { result } = hook({ maxFreq });
      await waitFor(() => expect(result.current.isSuccess).toBe(true));
    }
    expect(belts()).toHaveLength(2);
  });
});
