import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { RECONNECT_START_MS } from "@/pages/run/useLive";
import { setBase } from "@/test/http";
import { FakeSocket, stubWebSocket } from "@/test/websocket";

import { streamUrl, useStream } from "./useStream";

const stream = {
  type: "stream",
  chip: "adxl345",
  index: 2,
  rate_hz: 800,
  input_rate_hz: 3200,
  configured_rate_hz: 3200,
  factor: 4,
  max_hz: 200,
  updates_per_second: 10,
};

function frame(index: number): ArrayBuffer {
  const buffer = new ArrayBuffer(16 + 12);
  const view = new DataView(buffer);
  view.setUint16(0, index, true);
  view.setFloat64(4, 1.5, true);
  view.setUint32(12, 1, true);
  view.setFloat32(16, 0.25, true);
  return buffer;
}

beforeEach(() => {
  vi.useFakeTimers();
  stubWebSocket();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useStream", () => {
  it("builds a ws url below the base", () => {
    setBase("http://printer.local/ratos/");
    expect(streamUrl()).toBe("ws://printer.local/ratos/api/stream");
  });

  it("keeps the hello message", () => {
    const { result } = renderHook(() => useStream());
    const socket = FakeSocket.latest;
    expect(socket).toBeDefined();
    act(() => socket.open());
    const hello = {
      type: "hello",
      printing: false,
      defaults: { max_hz: 200, updates_per_second: 10 },
      limits: { max_updates_per_second: 30, max_hz: 1600, max_chips: 4 },
    };
    act(() => socket.send(hello));
    expect(result.current.state.connected).toBe(true);
    expect(result.current.state.hello).toMatchObject({ printing: false });
  });

  it("sends subscribe as JSON", () => {
    const { result } = renderHook(() => useStream());
    const socket = FakeSocket.latest;
    act(() => socket.open());
    act(() => result.current.subscribe("adxl345", 200, 10));
    expect(JSON.parse(socket.sent[0])).toEqual({
      type: "subscribe",
      chip: "adxl345",
      max_hz: 200,
      updates_per_second: 10,
    });
  });

  it("maps a frame index to the chip name", () => {
    const { result } = renderHook(() => useStream());
    const socket = FakeSocket.latest;
    expect(socket.binaryType).toBe("arraybuffer");
    act(() => socket.open());
    const seen: [string, number, number][] = [];
    result.current.onFrame((chip, f) => seen.push([chip, f.t0, f.xyz[0]]));
    act(() => socket.send(stream));
    act(() => socket.sendBinary(frame(2)));
    act(() => socket.sendBinary(frame(9)));
    expect(seen).toEqual([["adxl345", 1.5, 0.25]]);
    expect(result.current.state.chips.adxl345).toMatchObject({ index: 2, factor: 4, rateHz: 800 });
  });

  it("forgets a chip when it is unsubscribed", () => {
    const { result } = renderHook(() => useStream());
    const socket = FakeSocket.latest;
    act(() => socket.open());
    act(() => socket.send(stream));
    expect(result.current.state.chips.adxl345).toBeDefined();
    act(() => result.current.unsubscribe("adxl345"));
    expect(result.current.state.chips.adxl345).toBeUndefined();
  });

  it("subscribes again after a reconnect", () => {
    const { result } = renderHook(() => useStream());
    const first = FakeSocket.latest;
    act(() => first.open());
    act(() => result.current.subscribe("adxl345", 200, 10));
    act(() => first.drop());
    expect(result.current.state.connected).toBe(false);
    act(() => {
      vi.advanceTimersByTime(RECONNECT_START_MS);
    });
    const second = FakeSocket.latest;
    expect(second).not.toBe(first);
    act(() => second.open());
    expect(second.sent).toEqual(first.sent);
    expect(second.sent).toHaveLength(1);
  });

  it("keeps the last five console lines", () => {
    const { result } = renderHook(() => useStream());
    const socket = FakeSocket.latest;
    act(() => socket.open());
    act(() => socket.send({ type: "console", lines: ["1", "2", "3", "4"] }));
    act(() => socket.send({ type: "console", lines: ["5", "6", "7"] }));
    expect(result.current.state.console).toEqual(["3", "4", "5", "6", "7"]);
  });
});
