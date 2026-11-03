import { useQuery } from '@tanstack/react-query'

import { ApiClientError } from '@/lib/api/client'
import { queryKeys, shouldRetry } from '@/lib/api/queries'
import type { Axis } from '@/lib/generated/Axis'
import type { FitParams } from '@/lib/generated/FitParams'
import type { SessionMeta } from '@/lib/generated/SessionMeta'

import { getAnalysisClient, type AnalysisClient } from './client'

const NO_OVERRIDES: string[] = []

interface Args {
  sessionId: string
  /** `SessionDetail.meta`; the analysis waits until it is there. */
  meta: SessionMeta | undefined
  axis: Axis | null
  params: FitParams
  /** Chips to include although their health or coverage would exclude them. */
  overrides?: string[]
  client?: AnalysisClient
}

/**
 * The analysis of one axis under given fit parameters. It runs again each time the view opens
 * or the parameters change, and keeps nothing once nothing shows it (the sample bytes stay in
 * the worker, so a re-run costs no download).
 */
export function useSessionAnalysis({
  sessionId,
  meta,
  axis,
  params,
  overrides = NO_OVERRIDES,
  client = getAnalysisClient(),
}: Args) {
  return useQuery({
    queryKey: queryKeys.analysis(sessionId, axis ?? 'X', params, overrides),
    queryFn: ({ signal }) =>
      client.analyze({ sessionId, meta: meta!, axis: axis!, params, overrides }, signal),
    enabled: meta !== undefined && axis !== null,
    gcTime: 0,
    staleTime: 0,
    // A failed analysis is deterministic, so only a download that may have hit a blip is retried.
    retry: (count, error) => error instanceof ApiClientError && shouldRetry(count, error),
  })
}
