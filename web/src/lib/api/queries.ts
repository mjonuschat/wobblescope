import {
  keepPreviousData,
  type Query,
  QueryClient,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useCallback, useEffect, useState } from "react";

import type { Axis } from "@/lib/generated/Axis";
import type { FitParams } from "@/lib/generated/FitParams";
import type { SaveRequest } from "@/lib/generated/SaveRequest";
import type { SessionDetail } from "@/lib/generated/SessionDetail";
import type { Summary } from "@/lib/generated/Summary";
import type { ImportGroupState } from "@/pages/sessions/importModel";

import { getAnalysisClient } from "@/analysis/client";

import { ApiClientError } from "./client";
import {
  type SessionFilters,
  type StartTestRequest,
  checkChips,
  deleteResult,
  deleteSession,
  getChips,
  getResult,
  getSession,
  getSettings,
  getStatus,
  getStorage,
  importGroup,
  listResults,
  listSessions,
  getPrinter,
  homePrinter,
  pinSession,
  saveResult,
  startTest,
  stopPrinter,
} from "./endpoints";

export const queryKeys = {
  sessionLists: ["sessions", "list"] as const,
  sessionList: (filters: SessionFilters) => ["sessions", "list", filters] as const,
  session: (id: string) => ["sessions", "detail", id] as const,
  results: ["results"] as const,
  result: (id: string) => ["results", id] as const,
  storage: ["storage"] as const,
  settings: ["settings"] as const,
  chips: ["chips"] as const,
  status: ["status"] as const,
  printer: ["printer"] as const,
  analysesOf: (id: string) => ["analysis", id] as const,
  spectrogramsOf: (id: string) => ["spectrogram", id] as const,
  /** One chip's map at one probe point. `createdAt` and `resultId` tell what it was made from, as for an analysis. */
  spectrogram: (id: string, axis: Axis, chip: string, point: number, maxFreq: number, createdAt = "", resultId = "") =>
    ["spectrogram", id, axis, chip, point, maxFreq, createdAt, resultId] as const,
  beltsOf: (id: string) => ["belts", id] as const,
  /** One chip's belt comparison. `createdAt` and `resultId` tell what it was made from, as for an analysis. */
  belts: (id: string, chip: string, maxFreq: number, includeExcluded: boolean, createdAt = "", resultId = "") =>
    ["belts", id, chip, maxFreq, includeExcluded, createdAt, resultId] as const,
  /** `createdAt` tells a session written again under the same id from the one analysed before. */
  analysis: (id: string, axis: Axis, params: FitParams, overrides: string[] = [], createdAt = "", resultId = "") =>
    ["analysis", id, axis, params, overrides, createdAt, resultId] as const,
};

/** A daemon answer with a JSON error is final. Anything else may be a blip, so retry it twice. */
export function shouldRetry(failureCount: number, error: unknown): boolean {
  if (error instanceof ApiClientError && error.body) return false;
  return failureCount < 2;
}

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: { queries: { retry: shouldRetry, refetchOnWindowFocus: false } },
  });
}

export function useSessions(filters: SessionFilters = {}) {
  return useQuery({
    queryKey: queryKeys.sessionList(filters),
    queryFn: ({ signal }) => listSessions(filters, signal),
    // Keeps the table, and the filter popover in its header, mounted while a new filter loads.
    placeholderData: keepPreviousData,
  });
}

export function useSession(id: string) {
  return useQuery({
    queryKey: queryKeys.session(id),
    queryFn: ({ signal }) => getSession(id, signal),
  });
}

export function useStorage() {
  return useQuery({
    queryKey: queryKeys.storage,
    queryFn: ({ signal }) => getStorage(signal),
  });
}

export function useResults() {
  return useQuery({
    queryKey: queryKeys.results,
    queryFn: ({ signal }) => listResults(signal),
  });
}

export function useResult(id: string) {
  return useQuery({
    queryKey: queryKeys.result(id),
    queryFn: ({ signal }) => getResult(id, signal),
  });
}

/** How often the app asks the daemon for its status. */
const STATUS_POLL_MS = 5000;

/** The daemon's status, polled. The poll itself retries, so a failed request is not retried. */
export function useStatus() {
  return useQuery({
    queryKey: queryKeys.status,
    queryFn: ({ signal }) => getStatus(signal),
    refetchInterval: STATUS_POLL_MS,
    retry: false,
  });
}

/** A failed session or storage query that the daemon refused because storage was not open. */
function isWaitingForStorage(query: Query): boolean {
  const root = query.queryKey[0];
  return (
    query.state.status === "error" &&
    (root === "sessions" || root === "storage") &&
    query.state.error instanceof ApiClientError &&
    query.state.error.code === "storage_unavailable"
  );
}

/**
 * Fetches again every session and storage query that failed with `storage_unavailable` once the
 * daemon says storage is open. A JSON error is never retried (`shouldRetry`), so without this
 * the sessions page would stay on its waiting text.
 */
export function useRecoverStorage() {
  const queryClient = useQueryClient();
  const { data, dataUpdatedAt } = useStatus();
  const ready = data?.storage.state === "ready";
  // biome-ignore lint/correctness/useExhaustiveDependencies: dataUpdatedAt re-runs the effect once per status answer
  useEffect(() => {
    if (!ready) return;
    // Each failed query is fetched again once per status answer, so a daemon that says "ready"
    // and still refuses is asked again at the next poll, not in a loop.
    const tried = new Set<string>();
    const recover = () => {
      const failed = queryClient
        .getQueryCache()
        .findAll({ predicate: isWaitingForStorage })
        .filter((query) => !tried.has(query.queryHash));
      for (const query of failed) {
        tried.add(query.queryHash);
        void queryClient.invalidateQueries({ queryKey: query.queryKey, exact: true });
      }
    };
    recover();
    // A query can fail after the status answer that said ready arrived.
    return queryClient.getQueryCache().subscribe((event) => {
      if (event.type === "updated" && event.action.type === "error") {
        queueMicrotask(recover);
      }
    });
  }, [ready, dataUpdatedAt, queryClient]);
}

export function usePinSession() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, pinned }: { id: string; pinned: boolean }) => pinSession(id, pinned),
    onSuccess: (summary) => {
      queryClient.setQueryData<SessionDetail>(queryKeys.session(summary.id), (old) =>
        old ? { ...old, summary } : old,
      );
      void queryClient.invalidateQueries({ queryKey: queryKeys.sessionLists });
      void queryClient.invalidateQueries({ queryKey: queryKeys.storage });
    },
  });
}

export function useDeleteSession() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteSession(id),
    onSuccess: (_, id) => {
      queryClient.removeQueries({ queryKey: queryKeys.session(id) });
      // The id can be written again with other data, so nothing of the old session may stay.
      queryClient.removeQueries({ queryKey: queryKeys.analysesOf(id) });
      queryClient.removeQueries({ queryKey: queryKeys.spectrogramsOf(id) });
      queryClient.removeQueries({ queryKey: queryKeys.beltsOf(id) });
      getAnalysisClient().forget(id);
      void queryClient.invalidateQueries({ queryKey: queryKeys.sessionLists });
      void queryClient.invalidateQueries({ queryKey: queryKeys.storage });
    },
  });
}

export interface ImportGroupFailure {
  groupId: string;
  error: Error;
  /** The 0-based position in the group of the file the daemon named, or null. */
  entryIndex: number | null;
}

/** Sends groups one after another and keeps going after a failure; `run` never throws. */
export function useImportGroups() {
  const queryClient = useQueryClient();
  const [isRunning, setIsRunning] = useState(false);
  const run = useCallback(
    async (groups: ImportGroupState[], onStored: (groupId: string, summary: Summary) => void) => {
      setIsRunning(true);
      const failed: ImportGroupFailure[] = [];
      try {
        for (const group of groups) {
          try {
            onStored(group.id, await importGroup(group));
          } catch (caught) {
            const error = caught instanceof Error ? caught : new Error(String(caught));
            let entryIndex: number | null = null;
            if (error instanceof ApiClientError && error.body?.error === "invalid_csv") {
              const position = (error.body as { file?: number | null }).file;
              if (typeof position === "number" && position >= 1 && position <= group.entries.length) {
                entryIndex = position - 1;
              }
            }
            failed.push({ groupId: group.id, error, entryIndex });
          }
        }
      } finally {
        await Promise.all([
          queryClient.invalidateQueries({ queryKey: queryKeys.sessionLists }),
          queryClient.invalidateQueries({ queryKey: queryKeys.storage }),
        ]);
        setIsRunning(false);
      }
      return { failed };
    },
    [queryClient],
  );
  return { run, isRunning };
}

/** The printer's settings, read when the run screen opens. */
export function useSettings() {
  return useQuery({
    queryKey: queryKeys.settings,
    queryFn: ({ signal }) => getSettings(signal),
  });
}

/** The configured chips. A check updates them, so the list is read again when the screen opens. */
export function useChips() {
  return useQuery({
    queryKey: queryKeys.chips,
    queryFn: ({ signal }) => getChips(signal),
  });
}

/** Checks every chip and puts the answer into the chip list. */
export function useCheckChips() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => checkChips(),
    onSuccess: (chips) => {
      queryClient.setQueryData(queryKeys.chips, chips);
    },
  });
}

export function useStartTest() {
  return useMutation({ mutationFn: (request: StartTestRequest) => startTest(request) });
}

const BUSY_POLL_MS = 1000;

export function usePrinterState(enabled: boolean) {
  return useQuery({
    queryKey: queryKeys.printer,
    queryFn: ({ signal }) => getPrinter(signal),
    // A busy printer is watched closely so the Run button frees up soon after it settles.
    refetchInterval: (query) => (query.state.data?.busy ? BUSY_POLL_MS : STATUS_POLL_MS),
    retry: false,
    enabled,
  });
}

/** The emergency stop for the whole printer. Klipper shuts down, so the status is read again. */
export function useStopPrinter() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => stopPrinter(),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.status });
      void queryClient.invalidateQueries({ queryKey: queryKeys.printer });
    },
  });
}

/** The key that lets other components see that homing is in flight in this browser. */
export const homeMutationKey = ["printer", "home"] as const;

export function useHomePrinter() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationKey: homeMutationKey,
    mutationFn: () => homePrinter(),
    onMutate: () => void queryClient.invalidateQueries({ queryKey: queryKeys.printer }),
    onSettled: () => void queryClient.invalidateQueries({ queryKey: queryKeys.printer }),
  });
}

export function useSaveResult() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ sessionId, body }: { sessionId: string; body: SaveRequest }) => saveResult(sessionId, body),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.results });
      void queryClient.invalidateQueries({ queryKey: queryKeys.storage });
    },
  });
}

export function useDeleteResult() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteResult(id),
    onSuccess: (_, id) => {
      queryClient.removeQueries({ queryKey: queryKeys.result(id) });
      queryClient.removeQueries({ queryKey: queryKeys.analysesOf(`result:${id}`) });
      queryClient.removeQueries({ queryKey: queryKeys.spectrogramsOf(`result:${id}`) });
      queryClient.removeQueries({ queryKey: queryKeys.beltsOf(`result:${id}`) });
      getAnalysisClient().forget(`result:${id}`);
      void queryClient.invalidateQueries({ queryKey: queryKeys.results });
      void queryClient.invalidateQueries({ queryKey: queryKeys.storage });
    },
  });
}
