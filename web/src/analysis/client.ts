import type { Axis } from '@/lib/generated/Axis'
import type { FitParams } from '@/lib/generated/FitParams'
import type { SessionMeta } from '@/lib/generated/SessionMeta'
import { fetchSample as fetchSampleFile } from '@/lib/api/endpoints'

import type { AnalysisResult, WorkerRequest, WorkerResponse } from './protocol'

/** The part of a Web Worker the client uses, so tests can pass in a fake. */
export interface WorkerLike {
  postMessage(message: WorkerRequest, transfer: Transferable[]): void
  terminate(): void
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null
  onerror: ((event: ErrorEvent) => void) | null
}

export interface AnalysisJob {
  sessionId: string
  meta: SessionMeta
  axis: Axis
  params: FitParams
}

/** A queued request that a newer one replaced before it was sent. */
export class SupersededError extends Error {
  constructor() {
    super('A newer analysis request replaced this one')
    this.name = 'SupersededError'
  }
}

export interface AnalysisClient {
  analyze(job: AnalysisJob, signal?: AbortSignal): Promise<AnalysisResult>
  /** Drops what is held for a session id that may be written again. */
  forget(sessionId: string): void
}

interface Options {
  createWorker: () => WorkerLike
  fetchSample?: typeof fetchSampleFile
}

interface Queued {
  job: AnalysisJob
  signal?: AbortSignal
  resolve: (result: AnalysisResult) => void
  reject: (error: unknown) => void
}

function abortError(): DOMException {
  return new DOMException('The analysis was cancelled', 'AbortError')
}

/** The recordings of one axis, as (chip position, segment position, file name). */
function filesFor(meta: SessionMeta, axis: Axis) {
  return meta.segments.flatMap((segment, index) =>
    segment.axis === axis
      ? segment.files.map((file) => ({
          chip: file.chip,
          segment: index,
          name: file.sample_file,
        }))
      : [],
  )
}

/**
 * Owns the analysis worker. It fetches the sample files of the requested axis on the main
 * thread, sends the worker only the ones it lacks (as transferred buffers), and answers one
 * request at a time: a newer request replaces one still waiting.
 */
export function createAnalysisClient({
  createWorker,
  fetchSample = fetchSampleFile,
}: Options): AnalysisClient {
  let worker: WorkerLike | null = null
  let heldSession: string | null = null
  let held = new Set<string>()
  let nextId = 1
  const pending = new Map<
    number,
    { resolve: (r: AnalysisResult) => void; reject: (e: unknown) => void }
  >()
  let active: Queued | null = null
  let waiting: Queued | null = null

  function ensureWorker(): WorkerLike {
    if (worker) return worker
    const created = createWorker()
    created.onmessage = (event) => {
      const response = event.data
      const entry = pending.get(response.id)
      if (!entry) return // A late answer to a request nobody waits for.
      pending.delete(response.id)
      if (response.type === 'result') entry.resolve(response.result)
      else entry.reject(new Error(response.message))
    }
    created.onerror = () => {
      const failure = new Error('The analysis module failed to load')
      for (const entry of pending.values()) entry.reject(failure)
      pending.clear()
      created.terminate()
      worker = null
      heldSession = null
      held = new Set()
    }
    worker = created
    return created
  }

  async function run({ job, signal }: Queued): Promise<AnalysisResult> {
    if (signal?.aborted) throw abortError()
    const target = ensureWorker()
    const alreadyHeld = heldSession === job.sessionId ? held : new Set<string>()
    const missing = filesFor(job.meta, job.axis).filter((f) => !alreadyHeld.has(f.name))
    const buffers = await Promise.all(
      missing.map((f) => fetchSample(job.sessionId, f.chip, f.segment, signal)),
    )
    if (signal?.aborted) throw abortError()
    // The worker failed to load while the files were downloading, so nothing can answer.
    if (worker !== target) throw new Error('The analysis module failed to load')
    if (heldSession !== job.sessionId) {
      heldSession = job.sessionId
      held = new Set()
    }
    for (const f of missing) held.add(f.name)

    const id = nextId++
    const files = Object.fromEntries(missing.map((f, i) => [f.name, buffers[i]]))
    return new Promise<AnalysisResult>((resolve, reject) => {
      pending.set(id, { resolve, reject })
      signal?.addEventListener(
        'abort',
        () => {
          pending.delete(id)
          reject(abortError())
        },
        { once: true },
      )
      target.postMessage(
        {
          type: 'analyze',
          id,
          sessionId: job.sessionId,
          meta: job.meta,
          axis: job.axis,
          params: job.params,
          files,
        },
        buffers,
      )
    })
  }

  async function start(entry: Queued) {
    active = entry
    try {
      entry.resolve(await run(entry))
    } catch (error) {
      entry.reject(error)
    } finally {
      active = null
      const next = waiting
      waiting = null
      if (next) void start(next)
    }
  }

  return {
    analyze(job, signal) {
      return new Promise<AnalysisResult>((resolve, reject) => {
        if (signal?.aborted) {
          reject(abortError())
          return
        }
        const entry: Queued = { job, signal, resolve, reject }
        if (!active) {
          void start(entry)
          return
        }
        waiting?.reject(new SupersededError())
        waiting = entry
        signal?.addEventListener(
          'abort',
          () => {
            if (waiting === entry) {
              waiting = null
              reject(abortError())
            }
          },
          { once: true },
        )
      })
    },
    forget(sessionId) {
      if (heldSession === sessionId) {
        heldSession = null
        held = new Set()
      }
      worker?.postMessage({ type: 'forget', sessionId }, [])
    },
  }
}

let shared: AnalysisClient | null = null

/** The app's one client, which starts its worker on the first analysis. */
export function getAnalysisClient(): AnalysisClient {
  shared ??= createAnalysisClient({
    createWorker: () =>
      new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' }),
  })
  return shared
}
