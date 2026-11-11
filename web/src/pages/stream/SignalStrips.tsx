import { useEffect, useRef, useState } from "react";

import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { UnitInput } from "@/components/UnitInput";

import type { ChipSignal } from "./useSignalBuffers";

export interface StripChip {
  chip: string;
  color: string;
  signal: ChipSignal | undefined;
}

interface Props {
  chips: StripChip[];
  /** A paused view keeps showing the data it had and does not follow new data. */
  paused: boolean;
}

const AXES = ["x", "y", "z"] as const;
const WIDTH = 640;
const HEIGHT = 90;
/** The trail never shows more buffers than this. */
const MAX_TRAIL = 200;

function mean(values: Float32Array): number {
  if (values.length === 0) return 0;
  let sum = 0;
  for (const value of values) sum += value;
  return sum / values.length;
}

/** The largest absolute deviation from each buffer's own mean, or 0 when there are none. */
function peak(buffers: Float32Array[]): number {
  let max = 0;
  for (const buffer of buffers) {
    const center = mean(buffer);
    for (const value of buffer) max = Math.max(max, Math.abs(value - center));
  }
  return max;
}

/**
 * Draws the samples across the full canvas width, minus their own mean so the DC offset (gravity,
 * sensor bias) does not hide the vibration. Each trail buffer uses its own mean to stay centred.
 */
function line(context: CanvasRenderingContext2D, values: Float32Array, scale: number) {
  const center = mean(values);
  const step = WIDTH / Math.max(values.length - 1, 1);
  context.beginPath();
  values.forEach((value, i) => {
    const y = HEIGHT / 2 - ((value - center) / scale) * (HEIGHT / 2);
    if (i === 0) context.moveTo(0, y);
    else context.lineTo(i * step, y);
  });
  context.stroke();
}

/** The scale the strips of a chip would pick for themselves, as the largest of their three. */
function autoPeak(signal: ChipSignal | undefined): number {
  if (!signal) return 0;
  return Math.max(...AXES.map((axis) => peak([signal[axis], ...signal.trail[axis].slice(-MAX_TRAIL)])));
}

/**
 * Draws one axis. The recent frame buffers are a faint trail, each spanning the full width like an
 * oscilloscope's persistence, and the latest samples on top. `manual` of 0 or less means auto.
 */
function drawStrip(
  canvas: HTMLCanvasElement,
  signal: ChipSignal | undefined,
  axis: (typeof AXES)[number],
  color: string,
  manual: number,
) {
  const context = canvas.getContext("2d");
  if (!context) return;
  context.clearRect(0, 0, WIDTH, HEIGHT);
  if (!signal) return;
  const latest = signal[axis];
  const trail = signal.trail[axis].slice(-MAX_TRAIL);
  const scale = manual > 0 ? manual : Math.max(peak([latest, ...trail]), 1e-9);
  context.globalAlpha = 0.08;
  context.strokeStyle = color;
  context.lineWidth = 1;
  for (const buffer of trail) line(context, buffer, scale);
  context.globalAlpha = 1;
  line(context, latest, scale);
}

/** Three canvases per chip (X, Y, Z). Each chip has an automatic scale or a fixed one in mm/s². */
export function SignalStrips({ chips, paused }: Props) {
  const [drawn, setDrawn] = useState(chips);
  if (!paused && drawn !== chips) setDrawn(chips);
  const canvases = useRef(new Map<string, HTMLCanvasElement>());
  const [scales, setScales] = useState<Record<string, { auto: boolean; text: string }>>({});

  useEffect(() => {
    for (const { chip, color, signal } of drawn) {
      const fixed = scales[chip];
      const manual = fixed && !fixed.auto ? Number(fixed.text) : 0;
      for (const axis of AXES) {
        const canvas = canvases.current.get(`${chip}/${axis}`);
        if (canvas) drawStrip(canvas, signal, axis, color, Number.isFinite(manual) ? manual : 0);
      }
    }
  }, [drawn, scales]);

  return (
    <div className="space-y-4">
      {chips.map(({ chip, color, signal }) => (
        <section key={chip} aria-label={`${chip} signal`} className="space-y-1">
          <div className="flex items-center gap-2 text-sm">
            <span className="inline-block size-3 rounded-sm" style={{ background: color }} />
            <span className="font-medium">{chip}</span>
            <div className="ml-auto flex items-center gap-3">
              <div className="flex items-center gap-2">
                <Switch
                  id={`${chip}-auto-scale`}
                  aria-label={`${chip} auto scale`}
                  checked={scales[chip]?.auto ?? true}
                  onCheckedChange={(auto) =>
                    setScales((old) => ({
                      ...old,
                      [chip]: {
                        auto,
                        text: old[chip]?.text || String(Math.max(1, Math.round(autoPeak(signal)))),
                      },
                    }))
                  }
                />
                <Label htmlFor={`${chip}-auto-scale`}>Auto Scale</Label>
              </div>
              <UnitInput
                unit="mm/s²"
                min={0}
                id={`${chip}-scale`}
                aria-label={`${chip} scale`}
                className="w-36"
                disabled={scales[chip]?.auto ?? true}
                value={scales[chip]?.text ?? ""}
                onChange={(event) =>
                  setScales((old) => ({ ...old, [chip]: { auto: old[chip]?.auto ?? false, text: event.target.value } }))
                }
              />
            </div>
          </div>
          {AXES.map((axis) => (
            <div key={axis} className="relative">
              <canvas
                width={WIDTH}
                height={HEIGHT}
                role="img"
                aria-label={`${chip} ${axis.toUpperCase()} signal`}
                className="w-full rounded-md border bg-background"
                ref={(element) => {
                  if (element) canvases.current.set(`${chip}/${axis}`, element);
                  else canvases.current.delete(`${chip}/${axis}`);
                }}
              />
              <span className="pointer-events-none absolute top-1 left-2 text-xs text-muted-foreground">
                {axis.toUpperCase()}
              </span>
            </div>
          ))}
        </section>
      ))}
    </div>
  );
}
