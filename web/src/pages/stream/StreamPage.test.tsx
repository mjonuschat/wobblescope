import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import { createQueryClient } from "@/lib/api/queries";
import type { Spectrum } from "@/lib/generated/Spectrum";
import { chip, chipsSnapshot } from "@/test/capture";
import { apiPath, stubApiWithStatus } from "@/test/status";
import { FakeSocket, stubWebSocket } from "@/test/websocket";
import type { SpectrumResponse } from "@/stream/spectrumHandler";
import type { SpectrumWorkerLike } from "@/stream/useSpectra";
import { renderRoute } from "@/test/render";

import { StreamPage } from "./StreamPage";

// The chart is a canvas, so the test reads the props the page gives it.
vi.mock("./SpectrumChart", () => ({
  SpectrumChart: (props: { chips: unknown[]; maxHz: number }) => (
    <pre data-testid="spectrum-chart">{JSON.stringify({ chips: props.chips, maxHz: props.maxHz })}</pre>
  ),
}));
vi.mock("./SignalStrips", () => ({ SignalStrips: () => <div data-testid="strips" /> }));
vi.mock("./Spectrogram", () => ({ Spectrogram: () => <div data-testid="spectrogram" /> }));

const HELLO = {
  type: "hello",
  printing: false,
  defaults: { max_hz: 200, updates_per_second: 10 },
  limits: { max_updates_per_second: 20, max_hz: 800, max_chips: 4 },
};

function streamHeader(chipName: string, index: number) {
  return {
    type: "stream",
    chip: chipName,
    index,
    rate_hz: 800,
    input_rate_hz: 3200,
    configured_rate_hz: 3200,
    factor: 4,
    max_hz: 200,
    updates_per_second: 10,
  };
}

function health(chipName: string, over: Record<string, unknown> = {}) {
  return {
    type: "health",
    chip: chipName,
    measured_rate_hz: 3190,
    configured_rate_hz: 3200,
    overflows: 3,
    errors: 1,
    dropped_frames: 7,
    warnings: [],
    ...over,
  };
}

const spectrum = (level: number): Spectrum => ({
  freq_bins: [0, 10, 20],
  psd_x: [level, level, level],
  psd_y: [level, level, level],
  psd_z: [level, level, level],
  psd_sum: [level, level, level],
});

class FakeWorker implements SpectrumWorkerLike {
  onmessage: SpectrumWorkerLike["onmessage"] = null;
  posted: unknown[] = [];
  postMessage(message: unknown) {
    this.posted.push(message);
  }
  terminate() {}
  emit(message: SpectrumResponse) {
    this.onmessage?.({ data: message } as MessageEvent<SpectrumResponse>);
  }
}

let worker: FakeWorker;
const names = ["a1", "a2", "a3", "a4", "a5"];

function open(count = 2) {
  worker = new FakeWorker();
  stubApiWithStatus((call) =>
    apiPath(call) === "chips"
      ? { body: chipsSnapshot({ chips: names.slice(0, count).map((n) => chip(n)) }) }
      : { status: 404, body: { error: "not_found", message: "no" } },
  );
  render(
    <QueryClientProvider client={createQueryClient()}>
      <TooltipProvider delay={0}>
        <StreamPage createWorker={() => worker} />
      </TooltipProvider>
    </QueryClientProvider>,
  );
}

const socket = () => FakeSocket.latest;
const server = (message: unknown) => act(() => socket().send(message));
async function connect(hello: unknown = HELLO) {
  act(() => {
    socket().open();
    socket().send(hello);
  });
  await screen.findByLabelText("Stream a1");
}
const sent = () => socket().sent.map((m) => JSON.parse(m));
async function choose(label: string, option: string | RegExp) {
  fireEvent.click(screen.getByRole("combobox", { name: label }));
  const item = await screen.findByRole("option", { name: option });
  fireEvent.pointerDown(item);
  fireEvent.pointerUp(item);
  fireEvent.click(item);
}
const pick = (name: string) => fireEvent.click(screen.getByLabelText(`Stream ${name}`));
const chartProps = () => JSON.parse(screen.getByTestId("spectrum-chart").textContent ?? "");

describe("the stream page", () => {
  beforeEach(() => stubWebSocket());

  it("subscribes with the Hello defaults when a chip is picked", async () => {
    open();
    await connect();
    pick("a1");
    expect(sent()).toEqual([{ type: "subscribe", chip: "a1", max_hz: 200, updates_per_second: 10 }]);
  });

  it("offers no chip before Hello arrives, then subscribes with the Hello defaults", async () => {
    open();
    act(() => socket().open());
    await screen.findByText(/Waiting for the daemon/);
    expect(screen.queryByLabelText("Stream a1")).toBeNull();
    server(HELLO);
    pick("a1");
    expect(sent()).toEqual([{ type: "subscribe", chip: "a1", max_hz: 200, updates_per_second: 10 }]);
  });

  it("shows an empty state without a connection", async () => {
    open();
    expect(screen.getByText(/Not connected to the daemon/)).toBeTruthy();
  });

  it("disables a fifth chip with the reason", async () => {
    open(5);
    await connect();
    for (const n of names.slice(0, 4)) pick(n);
    expect(screen.getByRole("switch", { name: "Stream a5" }).getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByText("at most 4 chips at once")).toBeTruthy();
    expect(screen.getByRole("switch", { name: "Stream a1" }).getAttribute("aria-disabled")).not.toBe("true");
  });

  it("lists each chip in a table with its rate and the highest frequency it can show", async () => {
    open();
    await connect();
    for (const name of ["Chip", "Type", "Rate", "Max Frequency", "Status"]) {
      expect(screen.getByRole("columnheader", { name })).toBeTruthy();
    }
    expect(screen.getAllByText("3200 Hz").length).toBeGreaterThan(0);
    expect(screen.getAllByText("1600 Hz").length).toBeGreaterThan(0);
    expect(screen.queryByText("unchecked")).toBeNull();
  });

  it("unsubscribes when a chip is cleared", async () => {
    open();
    await connect();
    pick("a1");
    pick("a1");
    expect(sent()[1]).toEqual({ type: "unsubscribe", chip: "a1" });
  });

  it("passes the chosen spectrum axes to every chip", async () => {
    open();
    await connect();
    pick("a1");
    pick("a2");
    server(streamHeader("a1", 0));
    server(streamHeader("a2", 1));
    act(() => worker.emit({ type: "spectrum", chip: "a1", spectrum: spectrum(1), peaks: [] }));
    act(() => worker.emit({ type: "spectrum", chip: "a2", spectrum: spectrum(1), peaks: [] }));
    expect(chartProps().chips.map((c: { axes: string[] }) => c.axes)).toEqual([["sum"], ["sum"]]);
    fireEvent.click(screen.getByRole("button", { name: "X" }));
    fireEvent.click(screen.getByRole("button", { name: "Total" }));
    expect(chartProps().chips.map((c: { axes: string[] }) => c.axes)).toEqual([["x"], ["x"]]);
  });

  it("keeps at least one spectrum axis on", async () => {
    open();
    await connect();
    pick("a1");
    server(streamHeader("a1", 0));
    act(() => worker.emit({ type: "spectrum", chip: "a1", spectrum: spectrum(1), peaks: [] }));
    fireEvent.click(screen.getByRole("button", { name: "Total" }));
    expect(chartProps().chips[0].axes).toEqual(["sum"]);
  });

  it("sends Subscribe again when the range changes", async () => {
    open();
    await connect();
    pick("a1");
    await choose("Frequency Range", "400 Hz");
    expect(sent().at(-1)).toEqual({ type: "subscribe", chip: "a1", max_hz: 400, updates_per_second: 10 });
  });

  it("sends Subscribe again when the update rate changes", async () => {
    open();
    await connect();
    pick("a1");
    await choose("Refresh Rate", "5 Hz");
    expect(sent().at(-1)).toEqual({ type: "subscribe", chip: "a1", max_hz: 200, updates_per_second: 5 });
  });

  it("sends the daemon's limit for Max and leaves each chip to clamp it", async () => {
    open();
    await connect();
    pick("a1");
    await choose("Frequency Range", "Max");
    expect(sent().at(-1).max_hz).toBe(800);
  });

  it("resets the peak hold to the current spectrum", async () => {
    open();
    await connect();
    pick("a1");
    server(streamHeader("a1", 0));
    act(() => worker.emit({ type: "spectrum", chip: "a1", spectrum: spectrum(9), peaks: [] }));
    act(() => worker.emit({ type: "spectrum", chip: "a1", spectrum: spectrum(2), peaks: [] }));
    expect(chartProps().chips[0].peakHold).toEqual([9, 9, 9]);
    fireEvent.click(screen.getByRole("button", { name: "Reset Peak Hold" }));
    act(() => worker.emit({ type: "spectrum", chip: "a1", spectrum: spectrum(2), peaks: [] }));
    expect(chartProps().chips[0].peakHold).toEqual([2, 2, 2]);
  });

  it("keeps the chart as it was while paused", async () => {
    open();
    await connect();
    pick("a1");
    server(streamHeader("a1", 0));
    act(() => worker.emit({ type: "spectrum", chip: "a1", spectrum: spectrum(1), peaks: [] }));
    fireEvent.click(screen.getByRole("button", { name: "Pause" }));
    act(() => worker.emit({ type: "spectrum", chip: "a1", spectrum: spectrum(5), peaks: [] }));
    expect(chartProps().chips[0].spectrum.psd_sum).toEqual([1, 1, 1]);
    fireEvent.click(screen.getByRole("button", { name: "Resume" }));
    act(() => worker.emit({ type: "spectrum", chip: "a1", spectrum: spectrum(7), peaks: [] }));
    expect(chartProps().chips[0].spectrum.psd_sum).toEqual([7, 7, 7]);
  });

  it("starts with a one second spectrum window", async () => {
    open();
    await connect();
    pick("a1");
    server(streamHeader("a1", 0));
    const configure = worker.posted.find((m) => (m as { type: string }).type === "configure");
    expect(configure).toMatchObject({ chip: "a1", windowSeconds: 1 });
    expect(screen.getByRole("combobox", { name: "Averaging Time" }).textContent).toContain("Balanced");
  });

  it("stops feeding the worker while paused and keeps the socket", async () => {
    open();
    await connect();
    pick("a1");
    server(streamHeader("a1", 0));
    const frame = () => {
      const buffer = new ArrayBuffer(28);
      const view = new DataView(buffer);
      view.setUint16(0, 0, true);
      view.setUint32(12, 1, true);
      act(() => socket().sendBinary(buffer));
    };
    const frames = () => worker.posted.filter((m) => (m as { type: string }).type === "frame").length;
    frame();
    expect(frames()).toBe(1);
    fireEvent.click(screen.getByRole("button", { name: "Pause" }));
    frame();
    expect(frames()).toBe(1);
    expect(socket().closed).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Resume" }));
    frame();
    expect(frames()).toBe(2);
  });

  it("carries the selected index as the colour of a chip whose earlier chip has no spectrum", async () => {
    open();
    await connect();
    pick("a1");
    pick("a2");
    server(streamHeader("a1", 0));
    server(streamHeader("a2", 1));
    act(() => worker.emit({ type: "spectrum", chip: "a2", spectrum: spectrum(1), peaks: [] }));
    expect(chartProps().chips).toHaveLength(1);
    expect(chartProps().chips[0]).toMatchObject({ chip: "a2", colorIndex: 1 });
  });

  it("draws the spectrum to the highest range the chips announce", async () => {
    open();
    await connect();
    pick("a1");
    pick("a2");
    server({ ...streamHeader("a1", 0), max_hz: 1592 });
    server({ ...streamHeader("a2", 1), max_hz: 804 });
    act(() => worker.emit({ type: "spectrum", chip: "a1", spectrum: spectrum(1), peaks: [] }));
    expect(chartProps().maxHz).toBe(1592);
  });

  it("offers 100, 200, 400, 800 and Max", async () => {
    open();
    await connect();
    fireEvent.click(screen.getByRole("combobox", { name: "Frequency Range" }));
    const names = (await screen.findAllByRole("option")).map((o) => o.textContent);
    expect(names).toEqual(expect.arrayContaining(["100 Hz", "200 Hz", "400 Hz", "800 Hz", "Max"]));
  });

  it("says a print is running only while one is", async () => {
    open();
    await connect({ ...HELLO, printing: true });
    expect(screen.getByText("A print is running: showing a lower update rate")).toBeTruthy();
  });

  it("does not mention a print when none runs", async () => {
    open();
    await connect();
    expect(screen.queryByText(/A print is running/)).toBeNull();
  });

  it("turns only the closed chip red", async () => {
    open();
    await connect();
    pick("a1");
    pick("a2");
    server(streamHeader("a1", 0));
    server(streamHeader("a2", 1));
    server({ type: "closed", chip: "a1", reason: "the chip went away" });
    expect(screen.getByTestId("health-a1").dataset.tone).toBe("error");
    expect(screen.getByTestId("health-a1").getAttribute("aria-label")).toContain("Closed: the chip went away");
    expect(screen.getByTestId("health-a2").dataset.tone).toBe("ok");
  });

  it("shows the counts, and amber for a warning", async () => {
    open();
    await connect();
    pick("a1");
    server(streamHeader("a1", 0));
    server(health("a1", { warnings: ["rate is low"] }));
    const badge = screen.getByTestId("health-a1");
    expect(badge.dataset.tone).toBe("warning");
    const details = badge.getAttribute("aria-label") ?? "";
    expect(badge.textContent).toContain("3190 / 3200 Hz");
    expect(details).toContain("Rate: 3190 Hz of 3200 Hz");
    expect(details).toContain("Overflows: 3");
    expect(details).toContain("Errors: 1");
    expect(details).toContain("Dropped frames: 7");
    expect(details).toContain("rate is low");
  });

  it("shows the counts and warnings in a tooltip on the dot", async () => {
    open();
    await connect();
    pick("a1");
    server(streamHeader("a1", 0));
    server(health("a1", { warnings: ["rate is low"] }));
    fireEvent.focus(screen.getByTestId("health-a1"));
    expect(await screen.findByText(/Overflows: 3/)).toBeTruthy();
    expect(screen.getByText("rate is low")).toBeTruthy();
  });

  it("says not reported and not configured when the chip gives none", async () => {
    open();
    await connect();
    pick("a1");
    server(streamHeader("a1", 0));
    server(health("a1", { overflows: null, errors: null, configured_rate_hz: null }));
    const text = screen.getByTestId("health-a1").getAttribute("aria-label") ?? "";
    expect(text).toContain("Overflows: not reported");
    expect(text).toContain("Errors: not reported");
    expect(text).toContain("not configured");
  });

  it("shows a worker failure in the chip badge", async () => {
    open();
    await connect();
    pick("a1");
    server(streamHeader("a1", 0));
    act(() => worker.emit({ type: "error", chip: "a1", message: "wasm failed" }));
    const badge = screen.getByTestId("health-a1");
    expect(badge.dataset.tone).toBe("error");
    expect(badge.getAttribute("aria-label")).toContain("wasm failed");
  });

  it("shows console lines with errors in red and prefixes stripped", async () => {
    open();
    await connect();
    expect(screen.queryByLabelText("Klipper console")).toBeNull();
    server({ type: "console", lines: ["// info line", "echo: hello", "!! Move out of range"] });
    expect(screen.getByLabelText("Klipper console")).toBeTruthy();
    expect(screen.getByText("info line")).toBeTruthy();
    expect(screen.getByText("hello")).toBeTruthy();
    expect(screen.getByText("!! Move out of range").className).toContain("text-destructive");
  });

  it("has a Live View link under Measure in the sidebar", async () => {
    stubApiWithStatus(() => ({ status: 404, body: { error: "not_found", message: "no" } }));
    renderRoute("/sessions");
    const link = await screen.findByRole("link", { name: "Live View" });
    expect(link.getAttribute("href")).toBe("/stream");
  });
});
