import { QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

import { createQueryClient, queryKeys } from '@/lib/api/queries'
import { analysis, defaultParams, twoAxisMeta } from '@/test/fixtures'

import { createAnalysisClient, type WorkerLike } from './client'
import type { WorkerRequest, WorkerResponse } from './protocol'
import { useSessionAnalysis } from './useSessionAnalysis'

/** A worker that answers every analysis request at once. */
function setup() {
  const requests: WorkerRequest[] = []
  const worker: WorkerLike = {
    onmessage: null,
    onerror: null,
    terminate() {},
    postMessage(message) {
      requests.push(message)
      if (message.type !== 'analyze') return
      const response: WorkerResponse = {
        type: 'result',
        id: message.id,
        result: { analysis: analysis(), snippets: { worstCase: 's', perChip: ['s', null] } },
      }
      queueMicrotask(() => this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>))
    },
  }
  const fetchSample = vi.fn(async () => new ArrayBuffer(1))
  const client = createAnalysisClient({ createWorker: () => worker, fetchSample })
  const queryClient = createQueryClient()
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
  const hook = (params = defaultParams, overrides: string[] = [], createdAt?: string) => {
    const meta = { ...twoAxisMeta(), ...(createdAt ? { created_at: createdAt } : {}) }
    return renderHook(
      ({ p, o }) =>
        useSessionAnalysis({ sessionId: 's1', meta, axis: 'X', params: p, overrides: o, client }),
      { wrapper, initialProps: { p: params, o: overrides } },
    )
  }
  const analyses = () => requests.filter((r) => r.type === 'analyze').length
  return { hook, analyses, fetchSample, requests, queryClient }
}

describe('useSessionAnalysis', () => {
  it('never shows the previous parameters\u2019 result for new ones', async () => {
    const { hook } = setup()
    const { result, rerender } = hook()
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    rerender({ p: { ...defaultParams, scv: 7 }, o: [] })
    expect(result.current.data).toBeUndefined()
    expect(result.current.isPending).toBe(true)
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
  })

  it('serves a new mount from the cache without analysing or fetching again', async () => {
    const { hook, analyses, fetchSample } = setup()
    const first = hook()
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true))
    first.unmount()
    const second = hook()
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true))
    expect(analyses()).toBe(1)
    expect(fetchSample).toHaveBeenCalledTimes(2) // The first mount's two X files only.
  })

  it('serves earlier parameters from the cache when they come back', async () => {
    const { hook, analyses } = setup()
    const { result, rerender } = hook()
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    rerender({ p: { ...defaultParams, scv: 7 }, o: [] })
    await waitFor(() => expect(analyses()).toBe(2))
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    rerender({ p: defaultParams, o: [] })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(analyses()).toBe(2)
  })

  it('analyses again once the session\u2019s results were dropped', async () => {
    const { hook, analyses, queryClient } = setup()
    const first = hook()
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true))
    first.unmount()
    queryClient.removeQueries({ queryKey: queryKeys.analysesOf('s1') })
    const second = hook()
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true))
    expect(analyses()).toBe(2)
  })

  it('analyses again for a session written again under the same id', async () => {
    const { hook, analyses } = setup()
    const first = hook()
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true))
    first.unmount()
    const second = hook(defaultParams, [], '2026-11-01T00:00:00Z')
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true))
    expect(analyses()).toBe(2)
  })

  it('analyses again, with the chips named, when the overrides change', async () => {
    const { hook, analyses, requests } = setup()
    const { result, rerender } = hook()
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    rerender({ p: defaultParams, o: ['b'] })
    expect(result.current.data).toBeUndefined()
    await waitFor(() => expect(analyses()).toBe(2))
    const last = requests.filter((r) => r.type === 'analyze').at(-1)
    expect(last).toMatchObject({ overrides: ['b'] })
  })
})
