import { useQuery } from '@tanstack/react-query'

import { ApiClientError } from '@/lib/api/client'
import { queryKeys, shouldRetry } from '@/lib/api/queries'
import type { Axis } from '@/lib/generated/Axis'
import type { FitParams } from '@/lib/generated/FitParams'
import type { SessionMeta } from '@/lib/generated/SessionMeta'

import { getAnalysisClient, type AnalysisClient } from './client'

const NO_OVERRIDES: string[] = []

/**
 * How long a result stays after nothing shows it. A re-run takes about 0.3 s in the worker and
 * a result holds under 1 MB, so a few minutes keep a tab switch instant at a small cost.
 */
const CACHE_MS = 5 * 60_000

interface Args {
  sessionId: string
  /** A saved result to fetch the samples from; `sessionId` is then only the cache key. */
  resultId?: string
  /** `SessionDetail.meta`; the analysis waits until it is there. */
  meta: SessionMeta | undefined
  axis: Axis | null
  params: FitParams
  /** Chips to include although their health or coverage would exclude them. */
  overrides?: string[]
  client?: AnalysisClient
}

/**
 * The analysis of one axis under given fit parameters. A result is kept for a few minutes after
 * the view closes, so reopening it or going back to earlier parameters does not run the fit
 * again. Deleting a session drops its results.
 */
export function useSessionAnalysis({
  sessionId,
  resultId,
  meta,
  axis,
  params,
  overrides = NO_OVERRIDES,
  client = getAnalysisClient(),
}: Args) {
  return useQuery({
    queryKey: queryKeys.analysis(sessionId, axis ?? 'X', params, overrides, meta?.created_at, resultId),
    queryFn: ({ signal }) =>
      client.analyze(
        { sessionId, resultId, meta: meta!, axis: axis!, params, overrides },
        signal,
      ),
    enabled: meta !== undefined && axis !== null,
    gcTime: CACHE_MS,
    // The answer depends only on the key, so a cached result never goes stale.
    staleTime: Infinity,
    // A failed analysis is deterministic, so only a download that may have hit a blip is retried.
    retry: (count, error) => error instanceof ApiClientError && shouldRetry(count, error),
  })
}
