import { useEffect, useRef } from 'react'

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

/** Bins kept per spectrum for the colour scale. That is enough for percentiles and cheap to sort. */
const SAMPLES_PER_SPECTRUM = 32
/** The colour scale is recomputed once per this many spectra. */
const SCALE_EVERY = 10

interface Sample {
  at: number
  values: number[]
}

interface Drawn {
  /** The clock reading of the newest column, or null before the first one. */
  lastAt: number | null
  /** Times of the columns still inside the span. */
  times: number[]
  samples: Sample[]
  sinceScale: number
  low: number
  range: number
}

/** Dark blue, through purple and orange, to pale yellow. */
const RAMP: [number, number, number][] = [
  [10, 10, 60],
  [90, 20, 120],
  [200, 60, 60],
  [250, 170, 40],
  [255, 250, 200],
]

function ramp(t: number): [number, number, number] {
  const position = Math.min(Math.max(t, 0), 1) * (RAMP.length - 1)
  const lower = Math.min(Math.floor(position), RAMP.length - 2)
  const f = position - lower
  const [r, g, b] = RAMP[lower].map((v, i) => Math.round(v + (RAMP[lower + 1][i] - v) * f))
  return [r, g, b]
}

/** The value below which `fraction` of the sorted values lie. */
function percentile(sorted: number[], fraction: number): number {
  return sorted[Math.min(sorted.length - 1, Math.floor(fraction * sorted.length))]
}

function updateScale(drawn: Drawn) {
  const all = drawn.samples.flatMap((sample) => sample.values).sort((a, b) => a - b)
  drawn.low = percentile(all, 0.05)
  drawn.range = Math.max(percentile(all, 0.99) - drawn.low, 1e-9)
  drawn.sinceScale = 0
}

/**
 * Scrolls the canvas left by the time since the last column and draws only the new column.
 * Columns keep the colour scale they were drawn with, so the scale drifting does not repaint
 * history. A full redraw is only needed when the canvas is new, which starts as empty.
 */
function addColumn(canvas: HTMLCanvasElement, drawn: Drawn, at: number, logPsd: number[]) {
  const context = canvas.getContext('2d')
  if (!context) return
  const pixel = (time: number) => Math.round((time / SPAN_MS) * WIDTH)
  const shift = drawn.lastAt === null ? WIDTH : pixel(at) - pixel(drawn.lastAt)
  if (shift >= WIDTH || shift < 0) {
    context.clearRect(0, 0, WIDTH, HEIGHT)
    drawn.times = []
    drawn.samples = []
  } else if (shift > 0) {
    context.drawImage(canvas, -shift, 0)
    context.clearRect(WIDTH - shift, 0, shift, HEIGHT)
  }
  drawn.lastAt = at
  drawn.times = [...drawn.times.filter((time) => at - time <= SPAN_MS), at]
  drawn.samples = drawn.samples.filter((sample) => at - sample.at <= SPAN_MS)
  const stride = Math.ceil(logPsd.length / SAMPLES_PER_SPECTRUM)
  drawn.samples.push({ at, values: logPsd.filter((_, bin) => bin % stride === 0) })
  if (drawn.times.length === 1 || ++drawn.sinceScale >= SCALE_EVERY) updateScale(drawn)

  // As wide as the scroll, so each spectrum keeps its own columns instead of overwriting the last.
  const columnPx = shift >= WIDTH ? 1 : Math.max(shift, 1)
  const image = context.createImageData(columnPx, HEIGHT)
  for (let y = 0; y < HEIGHT; y++) {
    const bin = Math.floor(((HEIGHT - 1 - y) / HEIGHT) * logPsd.length)
    const [r, g, b] = ramp((logPsd[bin] - drawn.low) / drawn.range)
    for (let x = 0; x < columnPx; x++) image.data.set([r, g, b, 255], (y * columnPx + x) * 4)
  }
  context.putImageData(image, WIDTH - columnPx, 0)
  canvas.dataset.columns = String(drawn.times.length)
}

/** One chip's summed spectrum over the last minute, newest on the right, low frequencies at the bottom. */
export function Spectrogram({ chip, color, spectrum, now = () => performance.now() }: Props) {
  const canvas = useRef<HTMLCanvasElement>(null)
  const clock = useRef(now)
  useEffect(() => {
    clock.current = now
  }, [now])
  const drawn = useRef<Drawn>({ lastAt: null, times: [], samples: [], sinceScale: 0, low: 0, range: 1 })

  useEffect(() => {
    if (!spectrum || !canvas.current) return
    const logPsd = spectrum.psd_sum.map((v) => Math.log10(Math.max(v, 1e-12)))
    if (logPsd.length > 0) addColumn(canvas.current, drawn.current, clock.current(), logPsd)
  }, [spectrum])

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
        className="w-full"
      />
    </figure>
  )
}
