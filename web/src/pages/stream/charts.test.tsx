import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Spectrum } from '@/lib/generated/Spectrum'

import { chipColor } from './colors'
import { Spectrogram } from './Spectrogram'
import { SignalStrips } from './SignalStrips'
import type { ChipSignal } from './useSignalBuffers'

type Call = [string, ...unknown[]]
let calls: Call[] = []

/** Records the canvas calls the components use and throws on any other method. */
function fakeContext(): CanvasRenderingContext2D {
  const record = (name: string) => (...args: unknown[]) => {
    calls.push([name, ...args])
  }
  const context: Record<string, unknown> = {
    globalAlpha: 1,
    strokeStyle: '',
    lineWidth: 1,
    ...Object.fromEntries(['beginPath', 'moveTo', 'lineTo', 'stroke', 'clearRect', 'drawImage', 'putImageData'].map((n) => [n, record(n)])),
    createImageData: (width: number, height: number) => ({ width, height, data: new Uint8ClampedArray(width * height * 4) }),
  }
  return new Proxy(context, {
    get: (target, key: string) => {
      if (!(key in target)) throw new Error(`unexpected canvas method ${key}`)
      return target[key]
    },
  }) as unknown as CanvasRenderingContext2D
}

beforeEach(() => {
  calls = []
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(fakeContext() as never)
})
afterEach(() => vi.restoreAllMocks())

const count = (name: string) => calls.filter(([n]) => n === name).length

describe('chipColor', () => {
  it('is stable and distinct for four chips', () => {
    const colors = [0, 1, 2, 3].map(chipColor)
    expect(new Set(colors).size).toBe(4)
    expect(chipColor(2)).toBe(colors[2])
  })
})

const signal = (trail: number, level = 1): ChipSignal => {
  const buffer = () => Float32Array.from([0, level, -level])
  const buffers = () => Array.from({ length: trail }, buffer)
  return { x: buffer(), y: buffer(), z: buffer(), trail: { x: buffers(), y: buffers(), z: buffers() }, version: 1 }
}

describe('SignalStrips', () => {
  it('draws at most 200 trail buffers per axis, plus the solid line', () => {
    render(<SignalStrips chips={[{ chip: 'a', color: '#fff', signal: signal(300) }]} paused={false} />)
    expect(count('stroke')).toBe(3 * (200 + 1))
  })

  it('changes the plotted amplitude with a manual scale', () => {
    const chips = [{ chip: 'a', color: '#fff', signal: signal(0) }]
    render(<SignalStrips chips={chips} paused={false} />)
    const lineYs = () => calls.filter(([n]) => n === 'lineTo').map((c) => c[2] as number)
    const auto = lineYs()[0]
    calls = []
    fireEvent.change(screen.getByLabelText('a scale'), { target: { value: '10' } })
    expect(lineYs()[0]).not.toBe(auto)
    expect(lineYs()[0]).toBeCloseTo(45 - (1 / 10) * 45)
  })

  it('draws each trail buffer across the full width, whatever its length', () => {
    const short = signal(0)
    short.x = Float32Array.from([0, 1, 0, 1, 0, 1, 0])
    short.trail.x = [Float32Array.from([1, 2, 3])]
    render(<SignalStrips chips={[{ chip: 'a', color: '#fff', signal: short }]} paused={false} />)
    const strokes = calls.filter(([n]) => n === 'beginPath').length
    expect(strokes).toBeGreaterThan(0)
    const xs = calls.filter(([n]) => n === 'lineTo').map((c) => c[1] as number)
    // x axis: trail (3 samples) draws first, then the 7 latest samples; both end at the right edge
    expect(xs.slice(0, 2)).toEqual([320, 640])
    expect(xs.slice(2, 8)[5]).toBe(640)
  })

  it('keeps the drawn data and ignores new frames once paused', () => {
    const chips = (level: number) => [{ chip: 'a', color: '#fff', signal: signal(1, level) }]
    const { rerender } = render(<SignalStrips chips={chips(1)} paused={false} />)
    const drawnBefore = count('stroke')
    expect(drawnBefore).toBeGreaterThan(0)
    act(() => rerender(<SignalStrips chips={chips(2)} paused />))
    calls = []
    act(() => rerender(<SignalStrips chips={chips(3)} paused />))
    expect(calls.filter(([n]) => n === 'stroke' || n === 'clearRect')).toEqual([])
  })
})

describe('Spectrogram', () => {
  const spectrum = (psd_sum = [1, 10, 100]): Spectrum => ({
    freq_bins: psd_sum.map((_, i) => i),
    psd_x: psd_sum,
    psd_y: psd_sum,
    psd_z: psd_sum,
    psd_sum,
  })
  const columns = () => Number(screen.getByLabelText('a spectrogram').getAttribute('data-columns'))
  const props = (now: () => number, s: Spectrum) => (
    <Spectrogram chip="a" color="#fff" spectrum={s} now={now} />
  )
  const put = () => calls.filter(([n]) => n === 'putImageData') as [string, { data: Uint8ClampedArray; width: number }, number, number][]
  const pixel = (image: { data: Uint8ClampedArray; width: number }, row: number) => Array.from(image.data.slice(row * image.width * 4, row * image.width * 4 + 4))

  it('draws the newest column at the right edge and scrolls older ones left', () => {
    let time = 0
    const now = () => time
    const { rerender } = render(props(now, spectrum()))
    expect(put()[0].slice(2)).toEqual([640 - 1, 0])
    time = 6_000
    act(() => rerender(props(now, spectrum())))
    expect(calls.filter(([n]) => n === 'drawImage').map((c) => c[2])).toEqual([-64])
    expect(put()[1].slice(2)).toEqual([640 - 64, 0])
  })

  it('draws each column as wide as the scroll since the previous one, at least one pixel', () => {
    let time = 0
    const now = () => time
    const { rerender } = render(props(now, spectrum()))
    for (const at of [1_000, 1_000, 1_100]) {
      time = at
      act(() => rerender(props(now, spectrum([2, 20, 200]))))
    }
    expect(put().map((c) => c[1].width)).toEqual([1, 11, 1, 1])
    expect(put().map((c) => c[2])).toEqual([639, 629, 639, 639])
  })

  it('maps log10 power, from the 5th to the 99th percentile, onto the ramp', () => {
    render(props(() => 0, spectrum([1, 10, 100])))
    const image = put()[0][1]
    // bins 0..2 are log10 0..2: the lowest is the dark end, the highest the pale end, 1 is mid ramp
    expect(pixel(image, 159)).toEqual([10, 10, 60, 255])
    expect(pixel(image, 80)).toEqual([200, 60, 60, 255])
    expect(pixel(image, 0)).toEqual([255, 250, 200, 255])
  })

  it('drops columns older than 60 seconds and clears when everything is older', () => {
    let time = 0
    const now = () => time
    const { rerender } = render(props(now, spectrum()))
    time = 30_000
    act(() => rerender(props(now, spectrum())))
    expect(columns()).toBe(2)
    time = 70_000
    act(() => rerender(props(now, spectrum())))
    expect(columns()).toBe(2)
    time = 200_000
    calls = []
    act(() => rerender(props(now, spectrum())))
    expect(columns()).toBe(1)
    expect(calls.filter(([n]) => n === 'drawImage')).toEqual([])
  })
})
