import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Spectrum } from '@/lib/generated/Spectrum'

import { chipColor } from './colors'
import { Spectrogram } from './Spectrogram'
import { SignalStrips } from './SignalStrips'
import type { ChipSignal } from './useSignalBuffers'

type Call = [string, ...unknown[]]
let calls: Call[] = []

beforeEach(() => {
  calls = []
  const record = (name: string) => (...args: unknown[]) => {
    calls.push([name, ...args])
  }
  const context = new Proxy({} as Record<string, unknown>, {
    get: (target, key: string) => target[key] ?? record(key),
    set: (target, key: string, value) => {
      target[key] = value
      return true
    },
  })
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(context as unknown as CanvasRenderingContext2D)
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

  it('stops redrawing while paused and keeps its data', () => {
    const { rerender } = render(<SignalStrips chips={[{ chip: 'a', color: '#fff', signal: signal(1) }]} paused />)
    calls = []
    rerender(<SignalStrips chips={[{ chip: 'a', color: '#fff', signal: signal(5) }]} paused />)
    expect(count('stroke')).toBe(0)
  })
})

describe('Spectrogram', () => {
  const spectrum = (): Spectrum => ({
    freq_bins: [0, 1, 2],
    psd_x: [1, 1, 1],
    psd_y: [1, 1, 1],
    psd_z: [1, 1, 1],
    psd_sum: [1, 10, 100],
  })
  const columns = () => Number(screen.getByLabelText('a spectrogram').getAttribute('data-columns'))

  it('records a column per spectrum and drops those older than 60 seconds', () => {
    let time = 0
    const now = () => time
    const { rerender } = render(<Spectrogram chip="a" color="#fff" spectrum={spectrum()} now={now} />)
    time = 30_000
    rerender(<Spectrogram chip="a" color="#fff" spectrum={spectrum()} now={now} />)
    expect(columns()).toBe(2)
    time = 70_000
    act(() => rerender(<Spectrogram chip="a" color="#fff" spectrum={spectrum()} now={now} />))
    expect(columns()).toBe(2)
    time = 100_000
    rerender(<Spectrogram chip="a" color="#fff" spectrum={spectrum()} now={now} />)
    expect(columns()).toBe(2)
    expect(count('fillRect')).toBeGreaterThan(0)
  })
})
