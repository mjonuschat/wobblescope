import {
  type Query,
  QueryClient,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { useEffect } from 'react'

import type { Axis } from '@/lib/generated/Axis'
import type { FitParams } from '@/lib/generated/FitParams'
import type { SessionDetail } from '@/lib/generated/SessionDetail'

import { getAnalysisClient } from '@/analysis/client'

import { ApiClientError } from './client'
import {
  type ImportRequest,
  type SessionFilters,
  deleteSession,
  getSession,
  getStatus,
  getStorage,
  importCsv,
  listSessions,
  pinSession,
} from './endpoints'

export const queryKeys = {
  sessionLists: ['sessions', 'list'] as const,
  sessionList: (filters: SessionFilters) => ['sessions', 'list', filters] as const,
  session: (id: string) => ['sessions', 'detail', id] as const,
  storage: ['storage'] as const,
  status: ['status'] as const,
  analysesOf: (id: string) => ['analysis', id] as const,
  analysis: (id: string, axis: Axis, params: FitParams) =>
    ['analysis', id, axis, params] as const,
}

/** A daemon answer with a JSON error is final. Anything else may be a blip, so retry it twice. */
export function shouldRetry(failureCount: number, error: unknown): boolean {
  if (error instanceof ApiClientError && error.body) return false
  return failureCount < 2
}

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: { queries: { retry: shouldRetry, refetchOnWindowFocus: false } },
  })
}

export function useSessions(filters: SessionFilters = {}) {
  return useQuery({
    queryKey: queryKeys.sessionList(filters),
    queryFn: ({ signal }) => listSessions(filters, signal),
  })
}

export function useSession(id: string) {
  return useQuery({
    queryKey: queryKeys.session(id),
    queryFn: ({ signal }) => getSession(id, signal),
  })
}

export function useStorage() {
  return useQuery({
    queryKey: queryKeys.storage,
    queryFn: ({ signal }) => getStorage(signal),
  })
}

/** How often the app asks the daemon for its status. */
const STATUS_POLL_MS = 5000

/** The daemon's status, polled. The poll itself retries, so a failed request is not retried. */
export function useStatus() {
  return useQuery({
    queryKey: queryKeys.status,
    queryFn: ({ signal }) => getStatus(signal),
    refetchInterval: STATUS_POLL_MS,
    retry: false,
  })
}

/** A failed session or storage query that the daemon refused because storage was not open. */
function isWaitingForStorage(query: Query): boolean {
  const root = query.queryKey[0]
  return (
    query.state.status === 'error' &&
    (root === 'sessions' || root === 'storage') &&
    query.state.error instanceof ApiClientError &&
    query.state.error.code === 'storage_unavailable'
  )
}

/**
 * Fetches again every session and storage query that failed with `storage_unavailable` once the
 * daemon says storage is open. A JSON error is never retried (`shouldRetry`), so without this
 * the sessions page would stay on its waiting text.
 */
export function useRecoverStorage() {
  const queryClient = useQueryClient()
  const { data, dataUpdatedAt } = useStatus()
  const ready = data?.storage.state === 'ready'
  useEffect(() => {
    if (!ready) return
    // Each failed query is fetched again once per status answer, so a daemon that says "ready"
    // and still refuses is asked again at the next poll, not in a loop.
    const tried = new Set<string>()
    const recover = () => {
      const failed = queryClient
        .getQueryCache()
        .findAll({ predicate: isWaitingForStorage })
        .filter((query) => !tried.has(query.queryHash))
      for (const query of failed) {
        tried.add(query.queryHash)
        void queryClient.invalidateQueries({ queryKey: query.queryKey, exact: true })
      }
    }
    recover()
    // A query can fail after the status answer that said ready arrived.
    return queryClient.getQueryCache().subscribe((event) => {
      if (event.type === 'updated' && event.action.type === 'error') {
        queueMicrotask(recover)
      }
    })
  }, [ready, dataUpdatedAt, queryClient])
}

export function usePinSession() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, pinned }: { id: string; pinned: boolean }) => pinSession(id, pinned),
    onSuccess: (summary) => {
      queryClient.setQueryData<SessionDetail>(queryKeys.session(summary.id), (old) =>
        old ? { ...old, summary } : old,
      )
      void queryClient.invalidateQueries({ queryKey: queryKeys.sessionLists })
      void queryClient.invalidateQueries({ queryKey: queryKeys.storage })
    },
  })
}

export function useDeleteSession() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => deleteSession(id),
    onSuccess: (_, id) => {
      queryClient.removeQueries({ queryKey: queryKeys.session(id) })
      // The id can be written again with other data, so nothing of the old session may stay.
      queryClient.removeQueries({ queryKey: queryKeys.analysesOf(id) })
      getAnalysisClient().forget(id)
      void queryClient.invalidateQueries({ queryKey: queryKeys.sessionLists })
      void queryClient.invalidateQueries({ queryKey: queryKeys.storage })
    },
  })
}

export function useImportCsv() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (request: ImportRequest) => importCsv(request),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.sessionLists })
      void queryClient.invalidateQueries({ queryKey: queryKeys.storage })
    },
  })
}
