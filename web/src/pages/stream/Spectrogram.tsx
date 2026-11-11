import { useEffect, useRef, useState } from 'react'

import type { Spectrum } from '@/lib/generated/Spectrum'

interface Props {
  chip: string
  color: string
  /** The chip's latest spectrum. Each new object adds a column. */
  spectrum: Spectrum | null
  /** The clock in milliseconds. Tests pass their own. */
  now?: () => number
}

/** How much time the canvas shows, in milliseconds. */
export const SPAN_MS = 60_000
const WIDTH = 640
const HEIGHT = 160
const COLUMN_PX = 3

interface Column {
  at: number
  /** log10 of the summed PSD per bin. */
  values: Float32Array
}

/** Dark blue, through purple and orange, to pale yellow. */
const RAMP: [number, number, number][] = [
  [10, 10, 60],
  [90, 20, 120],
  [200, 60, 60],
  [250, 170, 40],
  [255, 250, 200],
]

function ramp(t: number): string {
  const position = Math.min(Math.max(t, 0), 1) * (RAMP.length - 1)
  const lower = Math.min(Math.floor(position), RAMP.length - 2)
  const f = position - lower
  const [r, g, b] = RAMP[lower].map((v, i) => Math.round(v + (RAMP[lower + 1][i] - v) * f))
  return `rgb(${r}, ${g}, ${b})`
}

/** The value below which `fraction` of the sorted values lie. */
function percentile(sorted: number[], fraction: number): number {
  return sorted[Math.min(sorted.length - 1, Math.floor(fraction * sorted.length))]
}

function draw(canvas: HTMLCanvasElement, columns: Column[], at: number) {
  const context = canvas.getContext('2d')
  if (!context) return
  context.clearRect(0, 0, WIDTH, HEIGHT)
  const all = columns.flatMap((column) => Array.from(column.values)).sort((a, b) => a - b)
  if (all.length === 0) return
  const low = percentile(all, 0.05)
  const range = Math.max(percentile(all, 0.99) - low, 1e-9)
  for (const { at: columnAt, values } of columns) {
    const x = WIDTH - ((at - columnAt) / SPAN_MS) * WIDTH - COLUMN_PX
    const row = HEIGHT / values.length
    values.forEach((value, bin) => {
      context.fillStyle = ramp((value - low) / range)
      context.fillRect(x, HEIGHT - (bin + 1) * row, COLUMN_PX, Math.ceil(row))
    })
  }
}

/** One chip's summed spectrum over the last minute, newest on the right, low frequencies at the bottom. */
export function Spectrogram({ chip, color, spectrum, now = () => performance.now() }: Props) {
  const canvas = useRef<HTMLCanvasElement>(null)
  const clock = useRef(now)
  useEffect(() => {
    clock.current = now
  }, [now])
  const [columns, setColumns] = useState<Column[]>([])

  useEffect(() => {
    if (!spectrum) return
    const at = clock.current()
    const values = Float32Array.from(spectrum.psd_sum, (v) => Math.log10(Math.max(v, 1e-12)))
    setColumns((old) => [...old.filter((column) => at - column.at <= SPAN_MS), { at, values }])
  }, [spectrum])

  useEffect(() => {
    if (canvas.current) draw(canvas.current, columns, clock.current())
  }, [columns])

  return (
    <figure className="space-y-1">
      <figcaption className="flex items-center gap-2 text-sm">
        <span className="inline-block size-3 rounded-sm" style={{ background: color }} />
        {chip}
      </figcaption>
      <canvas
        ref={canvas}
        width={WIDTH}
        height={HEIGHT}
        role="img"
        aria-label={`${chip} spectrogram`}
        data-columns={columns.length}
        className="w-full"
      />
    </figure>
  )
}
