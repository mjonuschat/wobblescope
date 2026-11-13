import { useQuery } from "@tanstack/react-query";

import { ApiClientError } from "@/lib/api/client";
import { queryKeys, shouldRetry } from "@/lib/api/queries";
import type { Axis } from "@/lib/generated/Axis";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import { type AnalysisClient, getAnalysisClient } from "./client";

/** How long a map stays after nothing shows it, so closing and reopening the panel is instant. */
const CACHE_MS = 5 * 60_000;

interface Args {
  sessionId: string;
  /** A saved result to fetch the samples from; `sessionId` is then only the cache key. */
  resultId?: string;
  /** `SessionDetail.meta`; the map waits until it is there. */
  meta: SessionMeta | undefined;
  axis: Axis | null;
  chip: string | null;
  point: number | null;
  /** The map keeps the frequencies up to this, in Hz. */
  maxFreq: number;
  /** The map is only made once this is true, so nothing is computed before it is asked for. */
  enabled: boolean;
  client?: AnalysisClient;
}

/**
 * The time-frequency map of one chip's recording at one probe point. It does not depend on the fit
 * settings, so it is made once per recording and kept for a few minutes.
 */
export function useSpectrogram({
  sessionId,
  resultId,
  meta,
  axis,
  chip,
  point,
  maxFreq,
  enabled,
  client = getAnalysisClient(),
}: Args) {
  return useQuery({
    queryKey: queryKeys.spectrogram(
      sessionId,
      axis ?? "X",
      chip ?? "",
      point ?? 0,
      maxFreq,
      meta?.created_at,
      resultId,
    ),
    queryFn: ({ signal }) => {
      // `enabled` keeps this from running without them.
      if (!meta || !axis || chip === null || point === null) throw new Error("The spectrogram needs a recording");
      return client.spectrogram({ sessionId, resultId, meta, axis, chip, point, maxFreq }, signal);
    },
    enabled: enabled && meta !== undefined && axis !== null && chip !== null && point !== null,
    gcTime: CACHE_MS,
    // The map depends only on the key, so a cached one never goes stale.
    staleTime: Infinity,
    retry: (count, error) => error instanceof ApiClientError && shouldRetry(count, error),
  });
}
