import {
  QueryClient,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'

import type { Axis } from '@/lib/generated/Axis'
import type { FitParams } from '@/lib/generated/FitParams'
import type { SessionDetail } from '@/lib/generated/SessionDetail'

import { ApiClientError } from './client'
import {
  type ImportRequest,
  type SessionFilters,
  deleteSession,
  getSession,
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
