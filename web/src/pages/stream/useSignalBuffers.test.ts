import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { DecodedFrame } from '@/stream/frames'
import type { StreamApi } from '@/stream/useStream'

import { useSignalBuffers } from './useSignalBuffers'

let handler: ((chip: string, frame: DecodedFrame) => void) | null = null
let frames: FrameRequestCallback[] = []

const stream = {
  state: { connected: true, hello: null, chips: {}, console: [] },
  subscribe: () => {},
  unsubscribe: () => {},
  onFrame: (h: (chip: string, frame: DecodedFrame) => void) => {
    handler = h
    return () => {
      handler = null
    }
  },
} satisfies StreamApi

const send = (chip: string, values: number[]) =>
  handler!(chip, { index: 0, t0: 0, xyz: new Float32Array(values) })

beforeEach(() => {
  frames = []
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => frames.push(cb))
  vi.stubGlobal('cancelAnimationFrame', () => {})
})
afterEach(() => vi.unstubAllGlobals())

describe('useSignalBuffers', () => {
  it('splits a frame into axes and keeps the latest samples', () => {
    const { result } = renderHook(() => useSignalBuffers(stream, { samples: 3 }))
    act(() => {
      send('a', [1, 2, 3, 4, 5, 6])
      send('a', [7, 8, 9, 10, 11, 12])
      frames.forEach((cb) => cb(0))
    })
    expect(Array.from(result.current.a.x)).toEqual([4, 7, 10])
    expect(Array.from(result.current.a.z)).toEqual([6, 9, 12])
    expect(result.current.a.version).toBe(2)
  })

  it('forgets a chip that leaves the stream', () => {
    const chipState = { index: 0, rateHz: 800, inputRateHz: 3200, configuredRateHz: 3200, factor: 4, maxHz: 200, health: null, closed: null, refused: null }
    const withChips = (chips: Record<string, typeof chipState>): StreamApi => ({
      ...stream,
      state: { ...stream.state, chips },
    })
    const { result, rerender } = renderHook(({ s }) => useSignalBuffers(s), {
      initialProps: { s: withChips({ a: chipState, b: chipState }) },
    })
    act(() => {
      send('a', [1, 2, 3])
      send('b', [4, 5, 6])
      frames.forEach((cb) => cb(0))
    })
    expect(Object.keys(result.current).sort()).toEqual(['a', 'b'])
    rerender({ s: withChips({ a: chipState }) })
    expect(Object.keys(result.current)).toEqual(['a'])
  })

  it('keeps one trail buffer per frame, the last trailBuffers of them', () => {
    const { result } = renderHook(() => useSignalBuffers(stream, { trailBuffers: 2 }))
    act(() => {
      for (let i = 0; i < 4; i++) send('a', [i, 0, 0])
      frames.forEach((cb) => cb(0))
    })
    expect(result.current.a.trail.x.map((b) => b[0])).toEqual([2, 3])
  })

  it('renders once for a burst of frames', () => {
    renderHook(() => useSignalBuffers(stream))
    send('a', [1, 2, 3])
    send('a', [1, 2, 3])
    expect(frames).toHaveLength(1)
  })
})
