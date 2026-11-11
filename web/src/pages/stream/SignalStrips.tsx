import { useEffect, useRef, useState } from 'react'

import type { ChipSignal } from './useSignalBuffers'

export interface StripChip {
  chip: string
  color: string
  signal: ChipSignal | undefined
}

interface Props {
  chips: StripChip[]
  /** A paused view keeps showing the data it had and does not follow new data. */
  paused: boolean
}

const AXES = ['x', 'y', 'z'] as const
const WIDTH = 640
const HEIGHT = 90
/** The trail never shows more buffers than this. */
const MAX_TRAIL = 200

/** The largest absolute value of the buffers, or 0 when there are none. */
function peak(buffers: Float32Array[]): number {
  let max = 0
  for (const buffer of buffers) for (const value of buffer) max = Math.max(max, Math.abs(value))
  return max
}

function line(context: CanvasRenderingContext2D, values: Float32Array, scale: number, step: number) {
  context.beginPath()
  values.forEach((value, i) => {
    const y = HEIGHT / 2 - (value / scale) * (HEIGHT / 2)
    if (i === 0) context.moveTo(0, y)
    else context.lineTo(i * step, y)
  })
  context.stroke()
}

/**
 * Draws one axis: the recent frame buffers as a faint trail, each from the left edge like an
 * oscilloscope's persistence, and the latest samples on top. `manual` of 0 or less means auto.
 */
function drawStrip(
  canvas: HTMLCanvasElement,
  signal: ChipSignal | undefined,
  axis: (typeof AXES)[number],
  color: string,
  manual: number,
) {
  const context = canvas.getContext('2d')
  if (!context) return
  context.clearRect(0, 0, WIDTH, HEIGHT)
  if (!signal) return
  const latest = signal[axis]
  const trail = signal.trail[axis].slice(-MAX_TRAIL)
  const scale = manual > 0 ? manual : Math.max(peak([latest, ...trail]), 1e-9)
  const step = WIDTH / Math.max(latest.length - 1, 1)
  context.globalAlpha = 0.08
  context.strokeStyle = color
  context.lineWidth = 1
  for (const buffer of trail) line(context, buffer, scale, step)
  context.globalAlpha = 1
  line(context, latest, scale, step)
}

/** Three canvases per chip (X, Y, Z) with a scale box: empty means automatic. */
export function SignalStrips({ chips, paused }: Props) {
  const [drawn, setDrawn] = useState(chips)
  if (!paused && drawn !== chips) setDrawn(chips)
  const canvases = useRef(new Map<string, HTMLCanvasElement>())
  const [scales, setScales] = useState<Record<string, string>>({})

  useEffect(() => {
    for (const { chip, color, signal } of drawn) {
      const manual = Number(scales[chip] ?? '')
      for (const axis of AXES) {
        const canvas = canvases.current.get(`${chip}/${axis}`)
        if (canvas) drawStrip(canvas, signal, axis, color, Number.isFinite(manual) ? manual : 0)
      }
    }
  }, [drawn, scales])

  return (
    <div className="space-y-4">
      {chips.map(({ chip, color }) => (
        <section key={chip} aria-label={`${chip} signal`} className="space-y-1">
          <div className="flex items-center gap-2 text-sm">
            <span className="inline-block size-3 rounded-sm" style={{ background: color }} />
            <span className="font-medium">{chip}</span>
            <label className="ml-auto flex items-center gap-1">
              Scale
              <input
                type="number"
                min={0}
                placeholder="auto"
                aria-label={`${chip} scale`}
                className="w-24 rounded border bg-transparent px-1"
                value={scales[chip] ?? ''}
                onChange={(event) => setScales((old) => ({ ...old, [chip]: event.target.value }))}
              />
            </label>
          </div>
          {AXES.map((axis) => (
            <canvas
              key={axis}
              width={WIDTH}
              height={HEIGHT}
              role="img"
              aria-label={`${chip} ${axis.toUpperCase()} signal`}
              className="w-full"
              ref={(element) => {
                if (element) canvases.current.set(`${chip}/${axis}`, element)
                else canvases.current.delete(`${chip}/${axis}`)
              }}
            />
          ))}
        </section>
      ))}
    </div>
  )
}
