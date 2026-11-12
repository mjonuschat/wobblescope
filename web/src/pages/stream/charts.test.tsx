import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Spectrum } from "@/lib/generated/Spectrum";

import { chipColor } from "./colors";
import { Spectrogram, frequencyTicks, timeTicks } from "./Spectrogram";
import { SignalStrips } from "./SignalStrips";
import type { ChipSignal } from "./useSignalBuffers";

type Call = [string, ...unknown[]];
let calls: Call[] = [];

/** Records the canvas calls the components use and throws on any other method. */
function fakeContext(): CanvasRenderingContext2D {
  const record =
    (name: string) =>
    (...args: unknown[]) => {
      calls.push([name, ...args]);
    };
  const context: Record<string, unknown> = {
    globalAlpha: 1,
    strokeStyle: "",
    lineWidth: 1,
    ...Object.fromEntries(
      ["beginPath", "moveTo", "lineTo", "stroke", "clearRect", "drawImage", "putImageData"].map((n) => [n, record(n)]),
    ),
    createImageData: (width: number, height: number) => ({
      width,
      height,
      data: new Uint8ClampedArray(width * height * 4),
    }),
  };
  return new Proxy(context, {
    get: (target, key: string) => {
      if (!(key in target)) throw new Error(`unexpected canvas method ${key}`);
      return target[key];
    },
  }) as unknown as CanvasRenderingContext2D;
}

beforeEach(() => {
  calls = [];
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(fakeContext() as never);
});
afterEach(() => vi.restoreAllMocks());

const count = (name: string) => calls.filter(([n]) => n === name).length;

describe("chipColor", () => {
  it("is stable and distinct for four chips", () => {
    const colors = [0, 1, 2, 3].map(chipColor);
    expect(new Set(colors).size).toBe(4);
    expect(chipColor(2)).toBe(colors[2]);
  });
});

const signal = (trail: number, level = 1): ChipSignal => {
  const buffer = () => Float32Array.from([0, level, -level]);
  const buffers = () => Array.from({ length: trail }, buffer);
  return { x: buffer(), y: buffer(), z: buffer(), trail: { x: buffers(), y: buffers(), z: buffers() }, version: 1 };
};

describe("SignalStrips", () => {
  it("draws at most 200 trail buffers per axis, plus the solid line", () => {
    render(<SignalStrips chips={[{ chip: "a", color: "#fff", signal: signal(300) }]} paused={false} />);
    expect(count("stroke")).toBe(3 * (200 + 1));
  });

  it("changes the plotted amplitude with a manual scale", () => {
    const chips = [{ chip: "a", color: "#fff", signal: signal(0) }];
    render(<SignalStrips chips={chips} paused={false} />);
    const lineYs = () => calls.filter(([n]) => n === "lineTo").map((c) => c[2] as number);
    const auto = lineYs()[0];
    fireEvent.click(screen.getByRole("switch", { name: "a auto scale" }));
    calls = [];
    fireEvent.change(screen.getByLabelText("a scale"), { target: { value: "10" } });
    expect(lineYs()[0]).not.toBe(auto);
    expect(lineYs()[0]).toBeCloseTo(45 - (1 / 10) * 45);
  });

  it("centres a line with a large constant offset on the canvas middle", () => {
    const offset = Float32Array.from({ length: 64 }, (_, i) => 9800 + 5 * Math.sin(i / 4));
    const biased = signal(1);
    biased.x = offset;
    biased.trail.x = [Float32Array.from(offset, (v) => v + 3)];
    render(<SignalStrips chips={[{ chip: "a", color: "#fff", signal: biased }]} paused={false} />);
    const ys = calls.filter(([n]) => n === "lineTo").map((c) => c[2] as number);
    // x axis: the trail (63 lineTo) then the solid line (63 lineTo)
    for (const lineYs of [ys.slice(0, 63), ys.slice(63, 126)]) {
      expect(Math.min(...lineYs)).toBeGreaterThanOrEqual(0);
      expect(Math.max(...lineYs)).toBeLessThanOrEqual(90);
      expect(lineYs.reduce((a, b) => a + b, 0) / lineYs.length).toBeGreaterThan(35);
      expect(lineYs.reduce((a, b) => a + b, 0) / lineYs.length).toBeLessThan(55);
    }
  });

  it("scales the deviation from the mean with a manual scale", () => {
    const biased = signal(0);
    biased.x = Float32Array.from([9800, 9810, 9790]);
    render(<SignalStrips chips={[{ chip: "a", color: "#fff", signal: biased }]} paused={false} />);
    fireEvent.click(screen.getByRole("switch", { name: "a auto scale" }));
    calls = [];
    fireEvent.change(screen.getByLabelText("a scale"), { target: { value: "20" } });
    const ys = calls.filter(([n]) => n === "lineTo").map((c) => c[2] as number);
    expect(ys[0]).toBeCloseTo(45 - (10 / 20) * 45);
    expect(ys[1]).toBeCloseTo(45 + (10 / 20) * 45);
  });

  it("starts on auto with the number box disabled, and fills it from the auto scale, rounded up to the next 100, when switched off", () => {
    const biased = signal(0);
    biased.x = Float32Array.from([100, 140, 60]);
    render(<SignalStrips chips={[{ chip: "a", color: "#fff", signal: biased }]} paused={false} />);
    const box = screen.getByLabelText("a scale") as HTMLInputElement;
    expect(box.disabled).toBe(true);
    expect(screen.getByText("mm/s²")).toBeTruthy();
    fireEvent.click(screen.getByRole("switch", { name: "a auto scale" }));
    expect(box.disabled).toBe(false);
    expect(box.value).toBe("100");
  });

  it("rounds the starting fixed scale up to the next 100", () => {
    const loud = signal(0);
    loud.x = Float32Array.from([0, 250, -250]);
    render(<SignalStrips chips={[{ chip: "a", color: "#fff", signal: loud }]} paused={false} />);
    fireEvent.click(screen.getByRole("switch", { name: "a auto scale" }));
    expect((screen.getByLabelText("a scale") as HTMLInputElement).value).toBe("300");
  });

  it("labels the X, Y and Z strips of each chip", () => {
    const chips = [
      { chip: "a", color: "#fff", signal: signal(0) },
      { chip: "b", color: "#000", signal: signal(0) },
    ];
    render(<SignalStrips chips={chips} paused={false} />);
    for (const chip of ["a", "b"]) {
      const region = within(screen.getByRole("region", { name: `${chip} signal` }));
      for (const axis of ["X", "Y", "Z"]) expect(region.getByText(axis)).toBeTruthy();
    }
  });

  it("draws each trail buffer across the full width, whatever its length", () => {
    const short = signal(0);
    short.x = Float32Array.from([0, 1, 0, 1, 0, 1, 0]);
    short.trail.x = [Float32Array.from([1, 2, 3])];
    render(<SignalStrips chips={[{ chip: "a", color: "#fff", signal: short }]} paused={false} />);
    const strokes = calls.filter(([n]) => n === "beginPath").length;
    expect(strokes).toBeGreaterThan(0);
    const xs = calls.filter(([n]) => n === "lineTo").map((c) => c[1] as number);
    // x axis. The trail (3 samples) draws first, then the 7 latest samples. Both end at the right edge
    expect(xs.slice(0, 2)).toEqual([320, 640]);
    expect(xs.slice(2, 8)[5]).toBe(640);
  });

  it("keeps the drawn data and ignores new frames once paused", () => {
    const chips = (level: number) => [{ chip: "a", color: "#fff", signal: signal(1, level) }];
    const { rerender } = render(<SignalStrips chips={chips(1)} paused={false} />);
    const drawnBefore = count("stroke");
    expect(drawnBefore).toBeGreaterThan(0);
    act(() => rerender(<SignalStrips chips={chips(2)} paused />));
    calls = [];
    act(() => rerender(<SignalStrips chips={chips(3)} paused />));
    expect(calls.filter(([n]) => n === "stroke" || n === "clearRect")).toEqual([]);
  });
});

describe("Spectrogram", () => {
  const spectrum = (psd_sum = [1, 10, 100]): Spectrum => ({
    freq_bins: psd_sum.map((_, i) => i),
    psd_x: psd_sum,
    psd_y: psd_sum,
    psd_z: psd_sum,
    psd_sum,
  });
  const columns = () => Number(screen.getByLabelText("a spectrogram").getAttribute("data-columns"));
  const props = (now: () => number, s: Spectrum) => <Spectrogram chip="a" color="#fff" spectrum={s} now={now} />;
  const put = () =>
    calls.filter(([n]) => n === "putImageData") as [
      string,
      { data: Uint8ClampedArray; width: number },
      number,
      number,
    ][];
  const pixel = (image: { data: Uint8ClampedArray; width: number }, row: number) =>
    Array.from(image.data.slice(row * image.width * 4, row * image.width * 4 + 4));

  it("draws the newest column at the right edge and scrolls older ones left", () => {
    let time = 0;
    const now = () => time;
    const { rerender } = render(props(now, spectrum()));
    expect(put()[0].slice(2)).toEqual([640 - 1, 0]);
    time = 6_000;
    act(() => rerender(props(now, spectrum())));
    expect(calls.filter(([n]) => n === "drawImage").map((c) => c[2])).toEqual([-64]);
    expect(put()[1].slice(2)).toEqual([640 - 64, 0]);
  });

  it("draws each column as wide as the scroll since the previous one, at least one pixel", () => {
    let time = 0;
    const now = () => time;
    const { rerender } = render(props(now, spectrum()));
    for (const at of [1_000, 1_000, 1_100]) {
      time = at;
      act(() => rerender(props(now, spectrum([2, 20, 200]))));
    }
    expect(put().map((c) => c[1].width)).toEqual([1, 11, 1, 1]);
    expect(put().map((c) => c[2])).toEqual([639, 629, 639, 639]);
  });

  it("maps log10 power, from the 5th to the 99th percentile, onto the ramp", () => {
    render(props(() => 0, spectrum([1, 10, 100])));
    const image = put()[0][1];
    // bins 0..2 are log10 0..2. The lowest is the dark end, the highest the pale end, 1 is mid ramp
    expect(pixel(image, 159)).toEqual([10, 10, 60, 255]);
    expect(pixel(image, 80)).toEqual([200, 60, 60, 255]);
    expect(pixel(image, 0)).toEqual([255, 250, 200, 255]);
  });

  it("drops columns older than 60 seconds and clears when everything is older", () => {
    let time = 0;
    const now = () => time;
    const { rerender } = render(props(now, spectrum()));
    time = 30_000;
    act(() => rerender(props(now, spectrum())));
    expect(columns()).toBe(2);
    time = 70_000;
    act(() => rerender(props(now, spectrum())));
    expect(columns()).toBe(2);
    time = 200_000;
    calls = [];
    act(() => rerender(props(now, spectrum())));
    expect(columns()).toBe(1);
    expect(calls.filter(([n]) => n === "drawImage")).toEqual([]);
  });
});

describe("Spectrogram axes", () => {
  it("labels frequencies in round steps, at most six", () => {
    expect(frequencyTicks(200)).toEqual([0, 50, 100, 150, 200]);
    expect(frequencyTicks(800)).toEqual([0, 200, 400, 600, 800]);
    expect(frequencyTicks(199.8)).toEqual([0, 50, 100, 150, 200]);
    expect(frequencyTicks(1600).length).toBeLessThanOrEqual(7);
  });

  it("labels the last minute every 5 seconds, oldest first", () => {
    const ticks = timeTicks();
    expect(ticks[0]).toBe(60);
    expect(ticks.at(-1)).toBe(0);
    expect(ticks).toHaveLength(13);
    expect(ticks.every((t, i) => i === 0 || ticks[i - 1] - t === 5)).toBe(true);
  });
});
