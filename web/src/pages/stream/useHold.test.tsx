import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { HoldRequest } from "@/lib/generated/HoldRequest";
import { setBase } from "@/test/http";
import { FakeSocket, stubWebSocket } from "@/test/websocket";

import { PING_INTERVAL_MS, holdUrl, useHold } from "./useHold";

const REQUEST: HoldRequest = {
  axis: "X",
  frequency: 60,
  accel_per_hz: 60,
  duration_seconds: 30,
  start_point: null,
};

const STATE = {
  type: "state",
  phase: "holding",
  elapsed_seconds: 1.5,
  frequency: 60,
  axis: "X",
  acceleration: 3600,
  accel_limit: 3600,
};

beforeEach(() => {
  vi.useFakeTimers();
  stubWebSocket();
});

afterEach(() => {
  vi.useRealTimers();
});

function started() {
  const hook = renderHook(() => useHold());
  act(() => hook.result.current.start(REQUEST));
  const socket = FakeSocket.latest;
  act(() => socket.open());
  return { hook, socket };
}

describe("useHold", () => {
  it("builds a ws url below the base", () => {
    setBase("http://printer.local/ratos/");
    expect(holdUrl()).toBe("ws://printer.local/ratos/api/hold");
  });

  it("sends the request and then pings every half second", () => {
    const { socket } = started();
    expect(JSON.parse(socket.sent[0])).toEqual({ type: "start", ...REQUEST });
    act(() => {
      vi.advanceTimersByTime(PING_INTERVAL_MS * 3 + 100);
    });
    const pings = socket.sent.filter((s) => JSON.parse(s).type === "ping");
    expect(pings).toHaveLength(3);
  });

  it("gives the id and follows the state", () => {
    const { hook, socket } = started();
    expect(hook.result.current.status).toBe("starting");
    act(() => socket.send({ type: "started", id: "hold-2026-11-11T10:00:00Z" }));
    expect(hook.result.current.status).toBe("running");
    expect(hook.result.current.id).toBe("hold-2026-11-11T10:00:00Z");
    act(() => socket.send(STATE));
    expect(hook.result.current.state).toMatchObject({ phase: "holding", frequency: 60 });
  });

  it("shows a rejected change and the hold goes on", () => {
    const { hook, socket } = started();
    act(() => socket.send({ type: "started", id: "hold-1" }));
    act(() => socket.send({ type: "rejected", message: "axis Y has no room" }));
    expect(hook.result.current.rejected).toBe("axis Y has no room");
    expect(hook.result.current.status).toBe("running");
    act(() => hook.result.current.set({ frequency: 70 }));
    expect(JSON.parse(socket.sent.at(-1) as string)).toEqual({ type: "set", frequency: 70 });
    expect(hook.result.current.rejected).toBeNull();
  });

  it("fills the failures from a refusal, closes and stays idle", () => {
    const { hook, socket } = started();
    act(() =>
      socket.send({
        type: "refused",
        failures: [
          { check: "homed", message: "home X, Y and Z first" },
          { check: "not_printing", message: "a print is paused" },
        ],
      }),
    );
    expect(hook.result.current.failures.map((f) => f.check)).toEqual(["homed", "not_printing"]);
    expect(hook.result.current.status).toBe("idle");
    expect(socket.closed).toBe(true);
  });

  it("stops the pings when the hold ends", () => {
    const { hook, socket } = started();
    act(() => socket.send({ type: "started", id: "hold-1" }));
    act(() => socket.send({ type: "ended", reason: "stopped", restore: "done" }));
    expect(hook.result.current.status).toBe("ended");
    expect(hook.result.current.ended).toEqual({ reason: "stopped", restore: "done" });
    const before = socket.sent.length;
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(socket.sent.length).toBe(before);
    expect(socket.closed).toBe(true);
  });

  it("sends stop and closes when the page is left during a hold", () => {
    const { hook, socket } = started();
    act(() => socket.send({ type: "started", id: "hold-1" }));
    hook.unmount();
    expect(socket.sent.some((s) => JSON.parse(s).type === "stop")).toBe(true);
    expect(socket.closed).toBe(true);
  });

  it("ends with a message when the socket drops during a hold", () => {
    const { hook, socket } = started();
    act(() => socket.send({ type: "started", id: "hold-1" }));
    act(() => socket.drop());
    expect(hook.result.current.status).toBe("ended");
    expect(hook.result.current.error).toMatch(/connection to the service was lost/i);
  });

  it("ignores messages it does not know", () => {
    const { hook, socket } = started();
    act(() => socket.send({ type: "mystery" }));
    act(() => socket.onmessage?.(new MessageEvent("message", { data: "not json" })));
    expect(hook.result.current.status).toBe("starting");
  });
});
