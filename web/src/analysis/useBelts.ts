import { useQuery } from "@tanstack/react-query";

import { ApiClientError } from "@/lib/api/client";
import { queryKeys, shouldRetry } from "@/lib/api/queries";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import { type AnalysisClient, getAnalysisClient } from "./client";

/** How long a comparison stays after nothing shows it. */
const CACHE_MS = 5 * 60_000;

interface Args {
  sessionId: string;
  /** A saved result to fetch the samples from; `sessionId` is then only the cache key. */
  resultId?: string;
  /** `SessionDetail.meta`; the comparison waits until it is there. */
  meta: SessionMeta | undefined;
  chip: string;
  /** The comparison keeps the frequencies up to this, in Hz. */
  maxFreq: number;
  /** Use a recording that its health or coverage would exclude. */
  includeExcluded: boolean;
  /** The comparison is only made once this is true. */
  enabled: boolean;
  client?: AnalysisClient;
}

/** The comparison of one chip's two belts. It depends only on its key, so it is made once and kept. */
export function useBelts({
  sessionId,
  resultId,
  meta,
  chip,
  maxFreq,
  includeExcluded,
  enabled,
  client = getAnalysisClient(),
}: Args) {
  return useQuery({
    queryKey: queryKeys.belts(sessionId, chip, maxFreq, includeExcluded, meta?.created_at, resultId),
    queryFn: ({ signal }) => {
      // `enabled` keeps this from running without it.
      if (!meta) throw new Error("The belt comparison needs a recording");
      return client.belts({ sessionId, resultId, meta, chip, maxFreq, includeExcluded }, signal);
    },
    enabled: enabled && meta !== undefined,
    gcTime: CACHE_MS,
    staleTime: Infinity,
    retry: (count, error) => error instanceof ApiClientError && shouldRetry(count, error),
  });
}
