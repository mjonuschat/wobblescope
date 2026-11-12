import { useEffect, useMemo, useRef } from "react";

import type { Spectrum } from "@/lib/generated/Spectrum";
import { type ColourScale, RAMP, type ScaleSource, SharedAutoScale } from "@/stream/spectrogramScale";

interface Props {
  chip: string;
  color: string;
  /** The chip's latest spectrum. Each new object adds a column. */
  spectrum: Spectrum | null;
  /** The clock in milliseconds. Tests pass their own. */
  now?: () => number;
  /** The colour scale, shared with the other chips. Without one the chip scales itself. */
  scale?: ScaleSource;
}

/** How much time the canvas shows, in milliseconds. */
export const SPAN_MS = 60_000;
/** The distance between ticks on the time axis, in seconds. */
const TIME_TICK_SECONDS = 5;
const WIDTH = 640;
const HEIGHT = 160;

/** The frequencies to label: a round step that gives at most six ticks from 0 up to `maxHz`, rounded, since the top bin sits just under a round figure. */
export function frequencyTicks(maxHz: number): number[] {
  const top = Math.round(maxHz);
  const step = [5, 10, 20, 25, 50, 100, 200, 250, 500, 1000].find((s) => top / s <= 6) ?? 1000;
  const ticks: number[] = [];
  for (let f = 0; f <= top; f += step) ticks.push(f);
  return ticks;
}

/** The times to label, in seconds before now, from the oldest to 0. */
export function timeTicks(): number[] {
  const count = SPAN_MS / 1000 / TIME_TICK_SECONDS;
  return Array.from({ length: count + 1 }, (_, i) => (count - i) * TIME_TICK_SECONDS);
}

interface Drawn {
  /** The clock reading of the newest column, or null before the first one. */
  lastAt: number | null;
  /** Times of the columns still inside the span. */
  times: number[];
}

function ramp(t: number): [number, number, number] {
  const position = Math.min(Math.max(t, 0), 1) * (RAMP.length - 1);
  const lower = Math.min(Math.floor(position), RAMP.length - 2);
  const f = position - lower;
  const [r, g, b] = RAMP[lower].map((v, i) => Math.round(v + (RAMP[lower + 1][i] - v) * f));
  return [r, g, b];
}

/**
 * Scrolls the canvas left by the time since the last column and draws only the new column.
 * Columns keep the colour scale they were drawn with, so the scale drifting does not repaint
 * history. A full redraw is only needed when the canvas is new, which starts as empty.
 */
function addColumn(canvas: HTMLCanvasElement, drawn: Drawn, at: number, logPsd: number[], scale: ColourScale) {
  const context = canvas.getContext("2d");
  if (!context) return;
  const pixel = (time: number) => Math.round((time / SPAN_MS) * WIDTH);
  const shift = drawn.lastAt === null ? WIDTH : pixel(at) - pixel(drawn.lastAt);
  if (shift >= WIDTH || shift < 0) {
    context.clearRect(0, 0, WIDTH, HEIGHT);
    drawn.times = [];
  } else if (shift > 0) {
    context.drawImage(canvas, -shift, 0);
    context.clearRect(WIDTH - shift, 0, shift, HEIGHT);
  }
  drawn.lastAt = at;
  drawn.times = [...drawn.times.filter((time) => at - time <= SPAN_MS), at];
  // As wide as the scroll, so each spectrum keeps its own columns instead of overwriting the last.
  const columnPx = shift >= WIDTH ? 1 : Math.max(shift, 1);
  const image = context.createImageData(columnPx, HEIGHT);
  for (let y = 0; y < HEIGHT; y++) {
    const bin = Math.floor(((HEIGHT - 1 - y) / HEIGHT) * logPsd.length);
    const [r, g, b] = ramp((logPsd[bin] - scale.low) / scale.range);
    for (let x = 0; x < columnPx; x++) image.data.set([r, g, b, 255], (y * columnPx + x) * 4);
  }
  context.putImageData(image, WIDTH - columnPx, 0);
  canvas.dataset.columns = String(drawn.times.length);
}

/** One chip's summed spectrum over the last minute, newest on the right, low frequencies at the bottom. */
export function Spectrogram({ chip, color, spectrum, now = () => performance.now(), scale }: Props) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const clock = useRef(now);
  useEffect(() => {
    clock.current = now;
  }, [now]);
  const drawn = useRef<Drawn>({ lastAt: null, times: [] });
  const own = useRef<ScaleSource>(new SharedAutoScale());
  const source = scale ?? own.current;

  useEffect(() => {
    if (!spectrum || !canvas.current) return;
    const logPsd = spectrum.psd_sum.map((v) => Math.log10(Math.max(v, 1e-12)));
    if (logPsd.length === 0) return;
    const at = clock.current();
    addColumn(canvas.current, drawn.current, at, logPsd, source.columnScale(chip, at, logPsd));
  }, [spectrum, chip, source]);

  const maxHz = spectrum?.freq_bins.at(-1) ?? 0;
  const frequencies = useMemo(() => (maxHz > 0 ? frequencyTicks(maxHz) : []), [maxHz]);
  const times = useMemo(timeTicks, []);

  return (
    <figure className="space-y-1">
      <figcaption className="flex items-center gap-2 text-sm">
        <span className="inline-block size-3 rounded-sm" style={{ background: color }} />
        {chip}
      </figcaption>
      <div className="grid grid-cols-[3.5rem_1fr] gap-x-1 pr-4 text-xs text-muted-foreground">
        <div className="relative" aria-hidden>
          {frequencies.map((f) => (
            <span
              key={f}
              className="absolute right-0 flex translate-y-1/2 items-center gap-1 tabular-nums"
              style={{ bottom: `${Math.min(f / maxHz, 1) * 100}%` }}
            >
              {f} Hz
              <span className="h-px w-1 bg-border" />
            </span>
          ))}
        </div>
        <canvas
          ref={canvas}
          width={WIDTH}
          height={HEIGHT}
          role="img"
          aria-label={`${chip} spectrogram`}
          className="block w-full rounded-md border bg-background"
        />
        <div />
        <div className="relative h-8" aria-hidden>
          {times.map((t) => (
            <span
              key={t}
              className="absolute top-0 flex -translate-x-1/2 flex-col items-center whitespace-nowrap tabular-nums"
              style={{ left: `${(1 - (t * 1000) / SPAN_MS) * 100}%` }}
            >
              <span className="h-1 w-px bg-border" />
              {t === 0 ? "0" : `-${t}`} s
            </span>
          ))}
        </div>
      </div>
    </figure>
  );
}
