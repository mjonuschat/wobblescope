import { useQuery } from "@tanstack/react-query";

import { ApiClientError } from "@/lib/api/client";
import { queryKeys, shouldRetry, useSession } from "@/lib/api/queries";

import { getAnalysisClient, type AnalysisClient } from "./client";

/** How long the outcomes stay after nothing shows them; see `useSessionAnalysis`. */
const CACHE_MS = 5 * 60_000;

/**
 * The per-chip outcomes of a calibration session. Loads the session's metadata, downloads each
 * recording on its own and analyses in the worker. A recording that cannot be downloaded makes
 * only its chip `Incomplete`.
 */
export function useAxesMapAnalysis(id: string, client: AnalysisClient = getAnalysisClient()) {
  const session = useSession(id);
  const meta = session.data?.meta;
  return useQuery({
    queryKey: queryKeys.axesMapAnalysis(id, meta?.created_at),
    queryFn: ({ signal }) => client.analyzeAxesMap({ sessionId: id, meta: meta! }, signal),
    enabled: meta !== undefined,
    gcTime: CACHE_MS,
    // The answer depends only on the key, so a cached result never goes stale.
    staleTime: Infinity,
    retry: (count, error) => error instanceof ApiClientError && shouldRetry(count, error),
  });
}
