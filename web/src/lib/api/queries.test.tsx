import { QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import { getAnalysisClient } from "@/analysis/client";
import { defaultParams } from "@/test/fixtures";

import type { ImportGroupState } from "@/pages/sessions/importModel";
import { stubFetch } from "@/test/http";

import {
  createQueryClient,
  queryKeys,
  useDeleteResult,
  useDeleteSession,
  useImportGroups,
  usePinSession,
  useSession,
} from "./queries";

function setup() {
  const client = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return { client, wrapper };
}

describe("query hooks", () => {
  it("updates the cached detail after a pin and invalidates lists and storage", async () => {
    const { client, wrapper } = setup();
    stubFetch({ body: { id: "s1", pinned: true } });
    client.setQueryData(queryKeys.session("s1"), { summary: { id: "s1", pinned: false }, meta: {} });
    client.setQueryData(queryKeys.sessionList({}), { sessions: [] });
    client.setQueryData(queryKeys.storage, {});
    const { result } = renderHook(() => usePinSession(), { wrapper });
    await act(() => result.current.mutateAsync({ id: "s1", pinned: true }));
    expect((client.getQueryData(queryKeys.session("s1")) as { summary: { pinned: boolean } }).summary.pinned).toBe(
      true,
    );
    expect(client.getQueryState(queryKeys.sessionList({}))?.isInvalidated).toBe(true);
    expect(client.getQueryState(queryKeys.storage)?.isInvalidated).toBe(true);
  });

  it("keys an analysis by its result so a result and a session never share one", () => {
    const session = queryKeys.analysis("s1", "X", defaultParams, [], "c");
    expect(queryKeys.analysis("s1", "X", defaultParams, [], "c", "r1")).not.toEqual(session);
    expect(queryKeys.analysis("s1", "X", defaultParams, [], "c", "r1")).not.toEqual(
      queryKeys.analysis("s1", "X", defaultParams, [], "c", "r2"),
    );
  });

  it("removes the result, its analyses and its worker files after a delete", async () => {
    const { client, wrapper } = setup();
    stubFetch({ status: 204 });
    client.setQueryData(queryKeys.result("r1"), {});
    client.setQueryData(queryKeys.analysis("result:r1", "X", defaultParams, [], "c", "r1"), {});
    const forget = vi.spyOn(getAnalysisClient(), "forget");
    const { result } = renderHook(() => useDeleteResult(), { wrapper });
    await act(() => result.current.mutateAsync("r1"));
    expect(client.getQueryData(queryKeys.result("r1"))).toBeUndefined();
    expect(client.getQueryData(queryKeys.analysis("result:r1", "X", defaultParams, [], "c", "r1"))).toBeUndefined();
    expect(forget).toHaveBeenCalledWith("result:r1");
  });

  it("removes the session and its analyses after a delete, forgets it in the worker and invalidates lists and storage", async () => {
    const { client, wrapper } = setup();
    stubFetch({ status: 204 });
    client.setQueryData(queryKeys.session("s1"), { summary: {}, meta: {} });
    client.setQueryData(queryKeys.analysis("s1", "X", defaultParams), { analysis: {} });
    const forget = vi.spyOn(getAnalysisClient(), "forget");
    client.setQueryData(queryKeys.sessionList({ pinned: true }), { sessions: [] });
    client.setQueryData(queryKeys.storage, {});
    const { result } = renderHook(() => useDeleteSession(), { wrapper });
    await act(() => result.current.mutateAsync("s1"));
    expect(client.getQueryData(queryKeys.session("s1"))).toBeUndefined();
    expect(client.getQueryData(queryKeys.analysis("s1", "X", defaultParams))).toBeUndefined();
    expect(forget).toHaveBeenCalledWith("s1");
    expect(client.getQueryState(queryKeys.sessionList({ pinned: true }))?.isInvalidated).toBe(true);
    expect(client.getQueryState(queryKeys.storage)?.isInvalidated).toBe(true);
  });

  it("reaches the error state after one attempt on a JSON 404", async () => {
    const { wrapper } = setup();
    const http = stubFetch({ status: 404, body: { error: "not_found", message: "gone" } });
    const { result } = renderHook(() => useSession("nope"), { wrapper });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(http.fn).toHaveBeenCalledTimes(1);
  });

  it("useImportGroups sends groups in order, reports each stored group, continues after a failed one and returns the failures", async () => {
    const { client, wrapper } = setup();
    const http = stubFetch(
      { status: 201, body: { id: "s1" } },
      { status: 400, body: { error: "invalid_csv", message: "bad", file: 2 } },
      { status: 400, body: { error: "invalid", message: "bad" } },
      { status: 201, body: { id: "s4" } },
    );
    client.setQueryData(queryKeys.sessionList({}), { sessions: [] });
    client.setQueryData(queryKeys.storage, {});
    const group = (id: string, count: number): ImportGroupState => ({
      id,
      createdAt: new Date("2026-11-12T10:00:00Z"),
      confirmed: true,
      entries: Array.from({ length: count }, (_, i) => ({
        id: `${id}-${i}`,
        file: new File(["x"], `${id}-${i}.csv`),
        chip: "adxl345",
        chipGuessed: false,
        axis: "X" as const,
        point: "",
        stampToken: null,
      })),
    });
    const stored: string[] = [];
    const { result } = renderHook(() => useImportGroups(), { wrapper });
    let outcome: Awaited<ReturnType<typeof result.current.run>> | undefined;
    await act(async () => {
      outcome = await result.current.run([group("a", 1), group("b", 2), group("c", 1), group("d", 1)], (id, summary) =>
        stored.push(`${id}:${summary.id}`),
      );
    });
    expect(http.fn).toHaveBeenCalledTimes(4);
    expect(stored).toEqual(["a:s1", "d:s4"]);
    expect(outcome?.failed.map((f) => [f.groupId, f.entryIndex])).toEqual([
      ["b", 1],
      ["c", null],
    ]);
    expect(client.getQueryState(queryKeys.sessionList({}))?.isInvalidated).toBe(true);
    expect(client.getQueryState(queryKeys.storage)?.isInvalidated).toBe(true);
    expect(result.current.isRunning).toBe(false);
  });
});
