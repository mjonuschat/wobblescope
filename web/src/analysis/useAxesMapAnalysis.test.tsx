import { QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import { createQueryClient } from "@/lib/api/queries";
import { calibrationFile, calibrationMeta, realWasm } from "@/test/calibration";
import { stubFetch } from "@/test/http";

import { createAnalysisClient, type WorkerLike } from "./client";
import { createHandler } from "./handler";
import type { WorkerResponse } from "./protocol";
import { useAxesMapAnalysis } from "./useAxesMapAnalysis";

/** A worker that runs the real handler over the real wasm build. */
function setup(fetchSample: (id: string, chip: number, segment: number) => Promise<ArrayBuffer>) {
  const handle = createHandler(async () => realWasm());
  const worker: WorkerLike = {
    onmessage: null,
    onerror: null,
    terminate() {},
    postMessage(message) {
      void handle(message).then((response) => {
        if (response) this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>);
      });
    },
  };
  const client = createAnalysisClient({ createWorker: () => worker, fetchSample });
  stubFetch({ body: { summary: { id: "cal1" }, meta: calibrationMeta() } });
  const queryClient = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
  return renderHook(() => useAxesMapAnalysis("cal1", client), { wrapper });
}

const outcomes = (chips: { chip: string; state: string; result?: { map: string | null } }[]) =>
  chips.map((c) => [c.chip, c.state, c.result?.map]);

describe("useAxesMapAnalysis", () => {
  it("returns_one_outcome_per_chip", async () => {
    const { result } = setup(async (_id, chip, segment) => calibrationFile(chip, segment));
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(outcomes(result.current.data!)).toEqual([
      ["a", "Analysed", "x, y, z"],
      ["b", "Analysed", "y, x, z"],
    ]);
  });

  it("a_failed_download_makes_only_its_chip_incomplete", async () => {
    const fetchSample = vi.fn(async (_id: string, chip: number, segment: number) => {
      if (chip === 1 && segment === 2) throw new Error("connection reset");
      return calibrationFile(chip, segment);
    });
    const { result } = setup(fetchSample);
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(outcomes(result.current.data!)).toEqual([
      ["a", "Analysed", "x, y, z"],
      ["b", "Incomplete", undefined],
    ]);
  });
  it("retries_a_transient_download_after_incomplete_result", async () => {
    let fail = true;
    const fetchSample = vi.fn(async (_id: string, chip: number, segment: number) => {
      if (fail && chip === 1 && segment === 2) throw new Error("connection reset");
      return calibrationFile(chip, segment);
    });
    const first = setup(fetchSample);
    await waitFor(() => expect(first.result.current.data?.[1]?.state).toBe("Incomplete"));
    expect(first.result.current.isStale).toBe(true);
    fail = false;
    await first.result.current.refetch();
    await waitFor(() => expect(first.result.current.data?.[1]?.state).toBe("Analysed"));
  });
});
