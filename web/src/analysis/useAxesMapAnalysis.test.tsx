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

/**
 * A worker that runs the real handler over the real wasm build, behind one query client, so a
 * hook can be mounted again as the page is when the user comes back to it.
 */
function harness(fetchSample: (id: string, chip: number, segment: number) => Promise<ArrayBuffer>) {
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
  return { client, mount: () => renderHook(() => useAxesMapAnalysis("cal1", client), { wrapper }) };
}

function setup(fetchSample: (id: string, chip: number, segment: number) => Promise<ArrayBuffer>) {
  return harness(fetchSample).mount();
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

  describe("when the user navigates away and comes back", () => {
    const downloads = (spy: { mock: { calls: unknown[][] } }, chip: number, segment: number) =>
      spy.mock.calls.filter(([, c, s]) => c === chip && s === segment).length;

    it("downloads_an_incomplete_chip_again_and_completes_it_once_the_failure_has_passed", async () => {
      let fail = true;
      const fetchSample = vi.fn(async (_id: string, chip: number, segment: number) => {
        if (fail && chip === 1 && segment === 2) throw new Error("connection reset");
        return calibrationFile(chip, segment);
      });
      const { mount } = harness(fetchSample);
      const first = mount();
      await waitFor(() => expect(first.result.current.data?.[1]?.state).toBe("Incomplete"));
      first.unmount();
      fail = false;
      const back = mount();
      await waitFor(() => expect(back.result.current.data?.[1]?.state).toBe("Analysed"));
      expect(outcomes(back.result.current.data!)).toEqual([
        ["a", "Analysed", "x, y, z"],
        ["b", "Analysed", "y, x, z"],
      ]);
      expect(downloads(fetchSample, 1, 2)).toBe(2);
    });

    it("keeps_an_incomplete_chip_incomplete_without_an_error_while_the_failure_lasts", async () => {
      const fetchSample = vi.fn(async (_id: string, chip: number, segment: number) => {
        if (chip === 1 && segment === 2) throw new Error("connection reset");
        return calibrationFile(chip, segment);
      });
      const { mount } = harness(fetchSample);
      const first = mount();
      await waitFor(() => expect(first.result.current.data?.[1]?.state).toBe("Incomplete"));
      first.unmount();
      const back = mount();
      await waitFor(() => expect(downloads(fetchSample, 1, 2)).toBe(2));
      await waitFor(() => expect(back.result.current.isFetching).toBe(false));
      expect(back.result.current.isError).toBe(false);
      expect(back.result.current.data?.[1]?.state).toBe("Incomplete");
    });

    it("does_not_analyse_a_complete_result_again", async () => {
      const fetchSample = vi.fn(async (_id: string, chip: number, segment: number) => calibrationFile(chip, segment));
      const { client, mount } = harness(fetchSample);
      // The worker keeps the files it has downloaded, so what shows a needless refetch is the analysis.
      const analyse = vi.spyOn(client, "analyzeAxesMap");
      const first = mount();
      await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
      expect(analyse).toHaveBeenCalledTimes(1);
      first.unmount();
      const back = mount();
      expect(back.result.current.data).toBeDefined();
      await waitFor(() => expect(back.result.current.isFetching).toBe(false));
      expect(analyse).toHaveBeenCalledTimes(1);
    });
  });
});
