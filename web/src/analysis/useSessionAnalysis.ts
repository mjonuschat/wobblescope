import { useQuery } from '@tanstack/react-query'

import { shouldRetry, queryKeys } from '@/lib/api/queries'
import type { Axis } from '@/lib/generated/Axis'
import type { FitParams } from '@/lib/generated/FitParams'
import type { SessionMeta } from '@/lib/generated/SessionMeta'

import { SupersededError, getAnalysisClient, type AnalysisClient } from './client'

interface Args {
  sessionId: string
  /** `SessionDetail.meta`; the analysis waits until it is there. */
  meta: SessionMeta | undefined
  axis: Axis | null
  params: FitParams
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
  client = getAnalysisClient(),
}: Args) {
  return useQuery({
    queryKey: queryKeys.analysis(sessionId, axis ?? 'X', params),
    queryFn: ({ signal }) =>
      client.analyze({ sessionId, meta: meta!, axis: axis!, params }, signal),
    enabled: meta !== undefined && axis !== null,
    gcTime: 0,
    staleTime: 0,
    retry: (count, error) => !(error instanceof SupersededError) && shouldRetry(count, error),
  })
}
