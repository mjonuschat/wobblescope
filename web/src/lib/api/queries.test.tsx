import { QueryClientProvider } from '@tanstack/react-query'
import { act, renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

import { getAnalysisClient } from '@/analysis/client'
import { defaultParams } from '@/test/fixtures'

import { stubFetch } from '@/test/http'

import {
  createQueryClient,
  queryKeys,
  useDeleteSession,
  usePinSession,
  useSession,
} from './queries'

function setup() {
  const client = createQueryClient()
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  )
  return { client, wrapper }
}

describe('query hooks', () => {
  it('updates the cached detail after a pin and invalidates lists and storage', async () => {
    const { client, wrapper } = setup()
    stubFetch({ body: { id: 's1', pinned: true } })
    client.setQueryData(queryKeys.session('s1'), { summary: { id: 's1', pinned: false }, meta: {} })
    client.setQueryData(queryKeys.sessionList({}), { sessions: [] })
    client.setQueryData(queryKeys.storage, {})
    const { result } = renderHook(() => usePinSession(), { wrapper })
    await act(() => result.current.mutateAsync({ id: 's1', pinned: true }))
    expect(
      (client.getQueryData(queryKeys.session('s1')) as { summary: { pinned: boolean } }).summary
        .pinned,
    ).toBe(true)
    expect(client.getQueryState(queryKeys.sessionList({}))?.isInvalidated).toBe(true)
    expect(client.getQueryState(queryKeys.storage)?.isInvalidated).toBe(true)
  })

  it('removes the session and its analyses after a delete, forgets it in the worker and invalidates lists and storage', async () => {
    const { client, wrapper } = setup()
    stubFetch({ status: 204 })
    client.setQueryData(queryKeys.session('s1'), { summary: {}, meta: {} })
    client.setQueryData(queryKeys.analysis('s1', 'X', defaultParams), { analysis: {} })
    const forget = vi.spyOn(getAnalysisClient(), 'forget')
    client.setQueryData(queryKeys.sessionList({ pinned: true }), { sessions: [] })
    client.setQueryData(queryKeys.storage, {})
    const { result } = renderHook(() => useDeleteSession(), { wrapper })
    await act(() => result.current.mutateAsync('s1'))
    expect(client.getQueryData(queryKeys.session('s1'))).toBeUndefined()
    expect(client.getQueryData(queryKeys.analysis('s1', 'X', defaultParams))).toBeUndefined()
    expect(forget).toHaveBeenCalledWith('s1')
    expect(client.getQueryState(queryKeys.sessionList({ pinned: true }))?.isInvalidated).toBe(true)
    expect(client.getQueryState(queryKeys.storage)?.isInvalidated).toBe(true)
  })

  it('reaches the error state after one attempt on a JSON 404', async () => {
    const { wrapper } = setup()
    const http = stubFetch({ status: 404, body: { error: 'not_found', message: 'gone' } })
    const { result } = renderHook(() => useSession('nope'), { wrapper })
    await waitFor(() => expect(result.current.isError).toBe(true))
    expect(http.fn).toHaveBeenCalledTimes(1)
  })
})
