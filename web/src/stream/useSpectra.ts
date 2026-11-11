import { useCallback, useEffect, useRef, useState } from 'react'

import type { Peak } from '@/lib/generated/Peak'
import type { Spectrum } from '@/lib/generated/Spectrum'

import type { SpectrumRequest, SpectrumResponse } from './spectrumHandler'
import type { StreamApi } from './useStream'

export interface ChipSpectrum {
  spectrum: Spectrum | null
  peaks: Peak[]
  /** The running maximum of `psd_sum` per bin. */
  peakHold: number[]
}

export type SpectraState = Record<string, ChipSpectrum>

/** The part of a Web Worker the hook uses, so tests can pass in a fake. */
export interface SpectrumWorkerLike {
  postMessage(message: SpectrumRequest, transfer?: Transferable[]): void
  terminate(): void
  onmessage: ((event: MessageEvent<SpectrumResponse>) => void) | null
}

function createSpectrumWorker(): SpectrumWorkerLike {
  return new Worker(new URL('./spectrumWorker.ts', import.meta.url), { type: 'module' })
}

interface Hold {
  values: number[]
  spacing: number
}

function spacingOf(spectrum: Spectrum): number {
  return spectrum.freq_bins.length > 1 ? spectrum.freq_bins[1] - spectrum.freq_bins[0] : 0
}

/**
 * Feeds the stream's frames to the spectrum worker and returns the latest spectrum, peaks and
 * peak hold per chip. A chip is configured when its stream header arrives, and again when its
 * rate or maximum frequency, `windowSeconds` or `updatesPerSecond` change.
 */
export function useSpectra(
  stream: StreamApi,
  windowSeconds: number,
  updatesPerSecond: number,
  createWorker: () => SpectrumWorkerLike = createSpectrumWorker,
): { spectra: SpectraState; resetPeakHold: (chip?: string) => void } {
  const [spectra, setSpectra] = useState<SpectraState>({})
  const workerRef = useRef<SpectrumWorkerLike | null>(null)
  const holds = useRef(new Map<string, Hold>())
  const configured = useRef(new Map<string, string>())
  const { chips } = stream.state
  const { onFrame } = stream

  useEffect(() => {
    const worker = createWorker()
    const known = configured.current
    workerRef.current = worker
    worker.onmessage = (event) => {
      const message = event.data
      if (message.type !== 'spectrum') return
      const { chip, spectrum, peaks } = message
      const spacing = spacingOf(spectrum)
      const old = holds.current.get(chip)
      const sameGrid =
        old && old.values.length === spectrum.psd_sum.length && Math.abs(old.spacing - spacing) < 1e-9
      const values = sameGrid
        ? spectrum.psd_sum.map((level, i) => Math.max(level, old.values[i]))
        : [...spectrum.psd_sum]
      holds.current.set(chip, { values, spacing })
      setSpectra((previous) => ({ ...previous, [chip]: { spectrum, peaks, peakHold: values } }))
    }
    const stop = onFrame((chip, frame) => {
      worker.postMessage({ type: 'frame', chip, t0: frame.t0, xyz: frame.xyz }, [frame.xyz.buffer as ArrayBuffer])
    })
    return () => {
      stop()
      worker.terminate()
      workerRef.current = null
      known.clear()
    }
  }, [onFrame, createWorker])

  useEffect(() => {
    const worker = workerRef.current
    if (!worker) return
    const minIntervalMs = 1000 / updatesPerSecond
    for (const [chip, state] of Object.entries(chips)) {
      if (state.rateHz <= 0 || state.maxHz <= 0) continue
      const key = `${state.rateHz}/${state.maxHz}/${windowSeconds}/${minIntervalMs}`
      if (configured.current.get(chip) === key) continue
      configured.current.set(chip, key)
      holds.current.delete(chip)
      worker.postMessage({
        type: 'configure',
        chip,
        rateHz: state.rateHz,
        windowSeconds,
        maxHz: state.maxHz,
        minIntervalMs,
      })
    }
  }, [chips, windowSeconds, updatesPerSecond, onFrame])

  const resetPeakHold = useCallback((chip?: string) => {
    if (chip === undefined) holds.current.clear()
    else holds.current.delete(chip)
    setSpectra((previous) => {
      const next: SpectraState = {}
      for (const [name, value] of Object.entries(previous)) {
        next[name] = chip === undefined || chip === name ? { ...value, peakHold: [] } : value
      }
      return next
    })
  }, [])

  return { spectra, resetPeakHold }
}
