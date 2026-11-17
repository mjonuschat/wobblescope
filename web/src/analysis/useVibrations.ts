import { useQuery } from "@tanstack/react-query";

import { ApiClientError } from "@/lib/api/client";
import { queryKeys, shouldRetry } from "@/lib/api/queries";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import { type AnalysisClient, getAnalysisClient } from "./client";

/** The highest frequency the profile keeps unless told otherwise, in Hz. */
const DEFAULT_MAX_FREQ = 1000;

/** How long a profile stays after nothing shows it. */
const CACHE_MS = 5 * 60_000;

interface Args {
  sessionId: string;
  /** A saved result to fetch the samples from. `sessionId` is then only the cache key. */
  resultId?: string;
  /** `SessionDetail.meta`. The profile waits until it is there. */
  meta: SessionMeta | undefined;
  /** The profile keeps the frequencies up to this, in Hz. */
  maxFreq?: number;
  /** Let a chip that its health or coverage would exclude take part in the worst case. */
  includeExcluded: boolean;
  /** The profile is only made once this is true. */
  enabled: boolean;
  client?: AnalysisClient;
}

/** The vibrations profile of a session. It depends only on its key, so it is made once and kept. */
export function useVibrations({
  sessionId,
  resultId,
  meta,
  maxFreq = DEFAULT_MAX_FREQ,
  includeExcluded,
  enabled,
  client = getAnalysisClient(),
}: Args) {
  return useQuery({
    queryKey: queryKeys.vibrations(sessionId, maxFreq, includeExcluded, meta?.created_at, resultId),
    queryFn: ({ signal }) => {
      // `enabled` keeps this from running without it.
      if (!meta) throw new Error("The vibrations profile needs a recording");
      return client.vibrations({ sessionId, resultId, meta, maxFreq, includeExcluded }, signal);
    },
    enabled: enabled && meta !== undefined,
    gcTime: CACHE_MS,
    staleTime: Infinity,
    retry: (count, error) => error instanceof ApiClientError && shouldRetry(count, error),
  });
}
