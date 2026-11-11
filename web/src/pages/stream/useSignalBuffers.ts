import { useEffect, useRef, useState } from 'react'

import type { StreamApi } from '@/stream/useStream'

export interface ChipSignal {
  x: Float32Array
  y: Float32Array
  z: Float32Array
  /** One buffer per received frame, newest last. */
  trail: { x: Float32Array[]; y: Float32Array[]; z: Float32Array[] }
  /** Grows with every frame, so a consumer can tell that the data changed. */
  version: number
}

export type SignalBuffers = Record<string, ChipSignal>

interface Options {
  /** The latest samples kept per axis. */
  samples?: number
  /** The most frame buffers kept in each trail. */
  trailBuffers?: number
}

const AXES = ['x', 'y', 'z'] as const

/** `old` followed by `added`, cut to the last `limit` values. */
function appendLatest(old: Float32Array, added: Float32Array, limit: number): Float32Array {
  const joined = new Float32Array(old.length + added.length)
  joined.set(old)
  joined.set(added, old.length)
  return joined.length > limit ? joined.slice(joined.length - limit) : joined
}

/**
 * Keeps the latest samples and a trail of recent frames per chip and axis. Frames go into a
 * ref; React state only holds a counter that moves at most once per animation frame, so a
 * burst of frames costs one render.
 */
export function useSignalBuffers(
  stream: StreamApi,
  { samples = 256, trailBuffers = 200 }: Options = {},
): SignalBuffers {
  const data = useRef<SignalBuffers>({})
  const [snapshot, setSnapshot] = useState<SignalBuffers>({})
  const { onFrame } = stream
  const { chips } = stream.state
  const seen = useRef(new Set<string>())

  // Forget a chip that was in the stream and has left it.
  useEffect(() => {
    const gone = [...seen.current].filter((chip) => !(chip in chips))
    seen.current = new Set(Object.keys(chips))
    if (gone.length === 0) return
    for (const chip of gone) delete data.current[chip]
    setSnapshot({ ...data.current })
  }, [chips])

  useEffect(() => {
    let pending: number | null = null
    const flush = () => {
      pending = null
      setSnapshot({ ...data.current })
    }
    const off = onFrame((chip, frame) => {
      const count = frame.xyz.length / 3
      const axes = AXES.map((_, a) => {
        const values = new Float32Array(count)
        for (let i = 0; i < count; i++) values[i] = frame.xyz[i * 3 + a]
        return values
      })
      const old = data.current[chip]
      const next: ChipSignal = {
        x: appendLatest(old?.x ?? new Float32Array(0), axes[0], samples),
        y: appendLatest(old?.y ?? new Float32Array(0), axes[1], samples),
        z: appendLatest(old?.z ?? new Float32Array(0), axes[2], samples),
        trail: {
          x: [...(old?.trail.x ?? []), axes[0]].slice(-trailBuffers),
          y: [...(old?.trail.y ?? []), axes[1]].slice(-trailBuffers),
          z: [...(old?.trail.z ?? []), axes[2]].slice(-trailBuffers),
        },
        version: (old?.version ?? 0) + 1,
      }
      data.current[chip] = next
      if (pending === null) pending = requestAnimationFrame(flush)
    })
    return () => {
      off()
      if (pending !== null) cancelAnimationFrame(pending)
    }
  }, [onFrame, samples, trailBuffers])

  return snapshot
}
