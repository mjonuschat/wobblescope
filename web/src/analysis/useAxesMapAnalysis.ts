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
    queryFn: ({ signal }) => {
      // `enabled` keeps this from running without it.
      if (!meta) throw new Error("The calibration analysis needs the session's metadata");
      return client.analyzeAxesMap({ sessionId: id, meta }, signal);
    },
    enabled: meta !== undefined,
    gcTime: CACHE_MS,
    // Incomplete results can include transient download failures and must be retried.
    staleTime: (query) => (query.state.data?.some((chip) => chip.state === "Incomplete") ? 0 : Infinity),
    retry: (count, error) => error instanceof ApiClientError && shouldRetry(count, error),
  });
}
