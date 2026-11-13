import { QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createQueryClient, queryKeys } from "@/lib/api/queries";
import type { FinishedTest } from "@/lib/generated/FinishedTest";
import type { TestState } from "@/lib/generated/TestState";
import { setBase } from "@/test/http";
import { FakeSocket, stubWebSocket } from "@/test/websocket";

import { RECONNECT_START_MS, liveUrl, useLive } from "./useLive";

const running: TestState = {
  id: "test-1",
  phase: "positioning",
  segment: 1,
  segments: 2,
  axis: "X",
  position: [150, 150, 20],
  expected_segment_seconds: 100,
  elapsed_seconds: 0,
  frequency_hz: null,
  frequency_range: [10, 135],
  chips: [{ chip: "adxl345", level: null, warnings: [] }],
};

const finished: FinishedTest = {
  id: "test-1",
  outcome: "complete",
  reason: null,
  code: null,
  session_id: "test-1",
  motion_uncertain: false,
};

function setup() {
  const client = createQueryClient();
  const invalidate = vi.spyOn(client, "invalidateQueries");
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  const hook = renderHook(() => useLive(), { wrapper });
  return { ...hook, invalidate };
}

beforeEach(() => {
  vi.useFakeTimers();
  stubWebSocket();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useLive", () => {
  it("follows the snapshot, a phase change, a level and the finished event in order", () => {
    const { result } = setup();
    const socket = FakeSocket.latest;
    act(() => socket.open());
    act(() => socket.send({ test: running, last: null }));
    expect(result.current).toMatchObject({ connected: true, known: true, test: { phase: "positioning" } });

    act(() => socket.send({ test: { ...running, phase: "measuring" }, last: null }));
    expect(result.current.test?.phase).toBe("measuring");

    const level: TestState = {
      ...running,
      phase: "measuring",
      chips: [{ chip: "adxl345", level: [1, 2, 3], warnings: [] }],
    };
    act(() => socket.send({ test: level, last: null }));
    expect(result.current.test?.chips[0].level).toEqual([1, 2, 3]);

    act(() => socket.send({ test: null, last: finished }));
    expect(result.current.test).toBeNull();
    expect(result.current.last).toEqual(finished);
  });

  it("opens the socket at the document base, keeping a path prefix", () => {
    setBase("http://printer.local/printers/a/");
    expect(liveUrl()).toBe("ws://printer.local/printers/a/api/live");
  });

  it("uses wss on an https page", () => {
    setBase("https://printer.local/printers/a/");
    expect(liveUrl()).toBe("wss://printer.local/printers/a/api/live");
  });

  it("marks the socket closed, reconnects after the wait and takes the new snapshot as the truth", () => {
    const { result } = setup();
    const first = FakeSocket.latest;
    act(() => first.open());
    act(() => first.send({ test: running, last: null }));

    act(() => first.drop());
    expect(result.current.connected).toBe(false);
    // The last state stays on screen until a snapshot replaces it.
    expect(result.current.test?.id).toBe("test-1");
    expect(FakeSocket.instances).toHaveLength(1);

    act(() => {
      vi.advanceTimersByTime(RECONNECT_START_MS);
    });
    expect(FakeSocket.instances).toHaveLength(2);
    const second = FakeSocket.latest;
    act(() => second.open());
    act(() => second.send({ test: null, last: finished }));
    expect(result.current).toMatchObject({ connected: true, test: null, last: { id: "test-1" } });
  });

  it("waits longer after each failed attempt", () => {
    setup();
    act(() => FakeSocket.latest.drop());
    act(() => {
      vi.advanceTimersByTime(RECONNECT_START_MS);
    });
    act(() => FakeSocket.latest.drop());
    act(() => {
      vi.advanceTimersByTime(RECONNECT_START_MS);
    });
    expect(FakeSocket.instances).toHaveLength(2);
    act(() => {
      vi.advanceTimersByTime(RECONNECT_START_MS);
    });
    expect(FakeSocket.instances).toHaveLength(3);
  });

  it("refreshes the sessions list and storage when a test finishes, but not for an old result", () => {
    const { invalidate } = setup();
    const socket = FakeSocket.latest;
    act(() => socket.open());
    // A test that ended before the page opened changed nothing the page cached.
    act(() => socket.send({ test: null, last: finished }));
    expect(invalidate).not.toHaveBeenCalled();

    act(() => socket.send({ test: { ...running, id: "test-2" }, last: finished }));
    act(() => socket.send({ test: null, last: { ...finished, id: "test-2", session_id: "test-2" } }));
    const keys = invalidate.mock.calls.map((call) => call[0]?.queryKey);
    expect(keys).toContainEqual(queryKeys.sessionLists);
    expect(keys).toContainEqual(queryKeys.storage);
  });

  it("closes the socket and stops reconnecting when the page unmounts", () => {
    const { unmount } = setup();
    const socket = FakeSocket.latest;
    unmount();
    expect(socket.closed).toBe(true);
    act(() => socket.drop());
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(FakeSocket.instances).toHaveLength(1);
  });
});
