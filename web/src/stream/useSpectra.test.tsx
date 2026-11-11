import { act, renderHook } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { Spectrum } from '@/lib/generated/Spectrum'

import type { SpectrumRequest, SpectrumResponse } from './spectrumHandler'
import { type SpectrumWorkerLike, useSpectra } from './useSpectra'
import type { ChipStreamState, StreamApi } from './useStream'
import type { DecodedFrame } from './frames'

function chipState(over: Partial<ChipStreamState> = {}): ChipStreamState {
  return {
    index: 0, rateHz: 800, inputRateHz: 3200, configuredRateHz: 3200, factor: 4, maxHz: 200,
    health: null, closed: null, refused: null, ...over,
  }
}

function spectrumOf(psd: number[], step = 1): Spectrum {
  const freq_bins = psd.map((_, i) => i * step)
  return { freq_bins, psd_x: psd, psd_y: psd, psd_z: psd, psd_sum: psd }
}

function setup() {
  const requests: SpectrumRequest[] = []
  const worker: SpectrumWorkerLike = {
    onmessage: null,
    terminate() {},
    postMessage(message) {
      requests.push(message)
    },
  }
  const handlers = new Set<(chip: string, frame: DecodedFrame) => void>()
  const onFrame: StreamApi['onFrame'] = (handler) => {
    handlers.add(handler)
    return () => handlers.delete(handler)
  }
  const api = (chips: Record<string, ChipStreamState>): StreamApi => ({
    state: { connected: true, hello: null, chips, console: [] },
    subscribe() {},
    unsubscribe() {},
    onFrame,
  })
  const reply = (chip: string, spectrum: Spectrum) =>
    act(() => {
      const response: SpectrumResponse = { type: 'spectrum', chip, spectrum, peaks: [] }
      worker.onmessage?.({ data: response } as MessageEvent<SpectrumResponse>)
    })
  const hook = (chips: Record<string, ChipStreamState>, windowSeconds = 2, ups = 10) =>
    renderHook(({ s, w, u }) => useSpectra(s, w, u, () => worker), {
      initialProps: { s: api(chips), w: windowSeconds, u: ups },
    })
  return { requests, reply, hook, api, handlers }
}

describe('useSpectra', () => {
  it('configures a chip with the update interval and forwards frames', () => {
    const { requests, hook, handlers } = setup()
    hook({ a: chipState() })
    expect(requests).toEqual([
      { type: 'configure', chip: 'a', rateHz: 800, windowSeconds: 2, maxHz: 200, minIntervalMs: 100 },
    ])
    const xyz = new Float32Array(3)
    for (const h of handlers) h('a', { index: 0, t0: 5, xyz })
    expect(requests[1]).toEqual({ type: 'frame', chip: 'a', t0: 5, xyz })
  })

  it('keeps a running maximum and clears it on reset', () => {
    const { reply, hook } = setup()
    const { result } = hook({ a: chipState(), b: chipState() })
    reply('a', spectrumOf([1, 5, 2]))
    reply('a', spectrumOf([3, 4, 2]))
    expect(result.current.spectra.a.peakHold).toEqual([3, 5, 2])
    reply('b', spectrumOf([7, 7, 7]))
    act(() => result.current.resetPeakHold('a'))
    expect(result.current.spectra.a.peakHold).toEqual([])
    expect(result.current.spectra.b.peakHold).toEqual([7, 7, 7])
    reply('a', spectrumOf([1, 1, 1]))
    expect(result.current.spectra.a.peakHold).toEqual([1, 1, 1])
    act(() => result.current.resetPeakHold())
    expect(result.current.spectra.b.peakHold).toEqual([])
  })

  it('restarts the maximum when the bin count or spacing changes', () => {
    const { reply, hook } = setup()
    const { result } = hook({ a: chipState() })
    reply('a', spectrumOf([9, 9, 9]))
    reply('a', spectrumOf([1, 1, 1], 2))
    expect(result.current.spectra.a.peakHold).toEqual([1, 1, 1])
    reply('a', spectrumOf([2, 2]))
    expect(result.current.spectra.a.peakHold).toEqual([2, 2])
  })

  it('configures again and resets the maximum when the header changes the rate', () => {
    const { requests, reply, hook, api } = setup()
    const { result, rerender } = hook({ a: chipState() })
    reply('a', spectrumOf([9, 9, 9]))
    rerender({ s: api({ a: chipState({ rateHz: 400 }) }), w: 2, u: 10 })
    expect(requests.filter((r) => r.type === 'configure')).toHaveLength(2)
    reply('a', spectrumOf([1, 1, 1]))
    expect(result.current.spectra.a.peakHold).toEqual([1, 1, 1])
  })

  it('configures again when the window or update rate changes, not otherwise', () => {
    const { requests, hook, api } = setup()
    const chips = { a: chipState() }
    const { rerender } = hook(chips)
    rerender({ s: api(chips), w: 2, u: 10 })
    expect(requests).toHaveLength(1)
    rerender({ s: api(chips), w: 4, u: 20 })
    expect(requests[1]).toMatchObject({ type: 'configure', windowSeconds: 4, minIntervalMs: 50 })
  })
})
