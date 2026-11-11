import { describe, expect, it, vi } from 'vitest'

import type { Spectrum } from '@/lib/generated/Spectrum'

import { createSpectrumHandler, type LiveSpectrumHandle, type SpectrumModule } from './spectrumHandler'

const spectrum: Spectrum = { freq_bins: [0, 1], psd_x: [1, 1], psd_y: [1, 1], psd_z: [1, 1], psd_sum: [3, 3] }

function setup() {
  const created: { args: number[]; handle: LiveSpectrumHandle & { pushes: [number, Float32Array][]; freed: boolean } }[] = []
  class Fake {
    pushes: [number, Float32Array][] = []
    freed = false
    constructor(...args: number[]) {
      created.push({ args, handle: this as never })
    }
    push(t0: number, xyz: Float32Array) {
      this.pushes.push([t0, xyz])
    }
    spectrum() {
      return spectrum
    }
    peaks() {
      return [{ freq: 1, level: 3 }]
    }
    free() {
      this.freed = true
    }
  }
  let clock = 0
  const handle = createSpectrumHandler(
    async () => ({ WasmLiveSpectrum: Fake }) as unknown as SpectrumModule,
    () => clock,
  )
  const configure = (chip: string, minIntervalMs = 100) =>
    handle({ type: 'configure', chip, rateHz: 800, windowSeconds: 2, maxHz: 200, minIntervalMs })
  const frame = (chip: string) => handle({ type: 'frame', chip, t0: 1, xyz: new Float32Array(3) })
  return { created, handle, configure, frame, advance: (ms: number) => (clock += ms) }
}

describe('createSpectrumHandler', () => {
  it('configures a window per chip and forwards frames', async () => {
    const { created, configure, frame } = setup()
    await configure('a')
    await configure('b')
    await frame('b')
    expect(created.map((c) => c.args)).toEqual([[800, 2, 200], [800, 2, 200]])
    expect(created[0].handle.pushes).toHaveLength(0)
    expect(created[1].handle.pushes).toHaveLength(1)
  })

  it('answers at most one spectrum per interval', async () => {
    const { configure, frame, advance } = setup()
    await configure('a')
    expect(await frame('a')).toEqual({ type: 'spectrum', chip: 'a', spectrum, peaks: [{ freq: 1, level: 3 }] })
    advance(50)
    expect(await frame('a')).toBeNull()
    advance(50)
    expect(await frame('a')).toMatchObject({ type: 'spectrum' })
  })

  it('frees a chip on drop and ignores its frames', async () => {
    const { created, configure, frame, handle } = setup()
    await configure('a')
    await handle({ type: 'drop', chip: 'a' })
    expect(created[0].handle.freed).toBe(true)
    expect(await frame('a')).toBeNull()
  })

  it('frees the old window when a chip is configured again', async () => {
    const { created, configure } = setup()
    await configure('a')
    await configure('a')
    expect(created[0].handle.freed).toBe(true)
    expect(created[1].handle.freed).toBe(false)
  })

  it('answers every request with the failure when the module does not load', async () => {
    const handle = createSpectrumHandler(async () => {
      throw new Error('no wasm')
    }, vi.fn())
    expect(await handle({ type: 'drop', chip: 'a' })).toEqual({ type: 'error', chip: 'a', message: 'no wasm' })
    expect(await handle({ type: 'frame', chip: 'b', t0: 0, xyz: new Float32Array(3) })).toEqual({
      type: 'error',
      chip: 'b',
      message: 'no wasm',
    })
  })
})
