import type { Axis } from "@/lib/generated/Axis";
import type { ChipsSnapshot } from "@/lib/generated/ChipsSnapshot";
import type { Homed } from "@/lib/generated/Homed";
import type { Mode } from "@/lib/generated/Mode";
import type { Overrides } from "@/lib/generated/Overrides";
import type { PrinterSettings } from "@/lib/generated/PrinterSettings";
import type { PrinterState } from "@/lib/generated/PrinterState";
import type { ResultDetail } from "@/lib/generated/ResultDetail";
import type { ResultList } from "@/lib/generated/ResultList";
import type { ResultSummary } from "@/lib/generated/ResultSummary";
import type { SaveRequest } from "@/lib/generated/SaveRequest";
import type { SessionDetail } from "@/lib/generated/SessionDetail";
import type { SessionList } from "@/lib/generated/SessionList";
import type { StartedTest } from "@/lib/generated/StartedTest";
import type { StatusResponse } from "@/lib/generated/StatusResponse";
import type { StopDispatched } from "@/lib/generated/StopDispatched";
import type { Summary } from "@/lib/generated/Summary";
import type { Usage } from "@/lib/generated/Usage";

import { parsePointText } from "@/lib/importFiles";
import type { ImportGroupState } from "@/pages/sessions/importModel";

import { ApiClientError, apiFetch, apiJson, apiUrl } from "./client";

/** The list filters the app offers. */
export interface SessionFilters {
  chip?: string;
  printer?: string;
  pinned?: boolean;
  /** The first day to include, as a local `YYYY-MM-DD`. */
  createdFrom?: string;
  /** The last day to include, as a local `YYYY-MM-DD`. */
  createdTo?: string;
}

function dayStart(day: string, offsetDays = 0): string | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(day)) return null;
  const [year, month, date] = day.split("-").map(Number);
  return new Date(year, month - 1, date + offsetDays).toISOString();
}

export function listSessions(filters: SessionFilters = {}, signal?: AbortSignal) {
  const url = apiUrl("sessions");
  if (filters.chip) url.searchParams.set("chip", filters.chip);
  if (filters.printer) url.searchParams.set("printer", filters.printer);
  if (filters.pinned !== undefined) url.searchParams.set("pinned", String(filters.pinned));
  const from = filters.createdFrom ? dayStart(filters.createdFrom) : null;
  // The daemon's `created_to` is exclusive, so the last day included ends where the next starts.
  const to = filters.createdTo ? dayStart(filters.createdTo, 1) : null;
  if (from) url.searchParams.set("created_from", from);
  if (to) url.searchParams.set("created_to", to);
  return apiJson<SessionList>(url, { signal });
}

export function getSession(id: string, signal?: AbortSignal) {
  return apiJson<SessionDetail>(`sessions/${encodeURIComponent(id)}`, { signal });
}

export function getStorage(signal?: AbortSignal) {
  return apiJson<Usage>("storage", { signal });
}

/** How long the status request may take before the app treats the daemon as not answering. */
export const STATUS_DEADLINE_MS = 4000;

/**
 * The daemon's status. A request that never settles would keep the previous healthy answer on
 * screen, because a query reuses a pending fetch, so it fails after [`STATUS_DEADLINE_MS`].
 */
export async function getStatus(signal?: AbortSignal): Promise<StatusResponse> {
  const controller = new AbortController();
  signal?.addEventListener("abort", () => controller.abort());
  let timer: ReturnType<typeof setTimeout> | undefined;
  const deadline = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      controller.abort();
      reject(new ApiClientError("The daemon did not answer in time", null));
    }, STATUS_DEADLINE_MS);
  });
  try {
    return await Promise.race([apiJson<StatusResponse>("status", { signal: controller.signal }), deadline]);
  } finally {
    clearTimeout(timer);
  }
}

export function pinSession(id: string, pinned: boolean) {
  return apiJson<Summary>(`sessions/${encodeURIComponent(id)}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ pinned }),
  });
}

export function deleteSession(id: string) {
  return apiJson<void>(`sessions/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export function importGroup(group: ImportGroupState, signal?: AbortSignal) {
  const files = group.entries.map((entry) => {
    const point = parsePointText(entry.point);
    if (point === "invalid") {
      throw new Error(`The point of ${entry.file.name} is not three numbers`);
    }
    return { chip: entry.chip.trim(), axis: entry.axis ? entry.axis.toLowerCase() : null, point };
  });
  const form = new FormData();
  const spec = { created_at: group.createdAt.toISOString(), files };
  form.append("session", new Blob([JSON.stringify(spec)], { type: "application/json" }), "session.json");
  for (const entry of group.entries) form.append("file", entry.file, entry.file.name);
  return apiJson<Summary>("imports", { method: "POST", body: form, signal });
}

/** The stored, compressed sample file of one recording, exactly as the daemon holds it. */
export async function fetchSample(
  id: string,
  chip: number,
  segment: number,
  signal?: AbortSignal,
): Promise<ArrayBuffer> {
  const response = await apiFetch(`sessions/${encodeURIComponent(id)}/samples/${chip}/${segment}`, { signal });
  return response.arrayBuffer();
}

export function saveResult(sessionId: string, body: SaveRequest) {
  return apiJson<ResultSummary>(`sessions/${encodeURIComponent(sessionId)}/results`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function listResults(signal?: AbortSignal) {
  return apiJson<ResultList>("results", { signal });
}

export function getResult(id: string, signal?: AbortSignal) {
  return apiJson<ResultDetail>(`results/${encodeURIComponent(id)}`, { signal });
}

export function deleteResult(id: string) {
  return apiJson<void>(`results/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export async function fetchResultSample(
  id: string,
  chip: number,
  segment: number,
  signal?: AbortSignal,
): Promise<ArrayBuffer> {
  const response = await apiFetch(`results/${encodeURIComponent(id)}/samples/${chip}/${segment}`, { signal });
  return response.arrayBuffer();
}

/** The printer's effective `[resonance_tester]` settings and the toolhead's travel. */
export function getSettings(signal?: AbortSignal) {
  return apiJson<PrinterSettings>("settings", { signal });
}

/** The printer's accelerometers and what the daemon knows of each. */
export function getChips(signal?: AbortSignal) {
  return apiJson<ChipsSnapshot>("chips", { signal });
}

/** Opens every chip, waits for samples and answers with the updated chip list. */
export function checkChips() {
  return apiJson<ChipsSnapshot>("chips/check", { method: "POST" });
}

/** What the run form sends. Only the settings the user changed are in `overrides`. */
export interface StartTestRequest {
  chips: string[];
  axes: Axis[];
  mode: Mode;
  overrides: Partial<Overrides>;
  points: [number, number, number][];
}

export function startTest(request: StartTestRequest) {
  return apiJson<StartedTest>("tests", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
  });
}

export function getPrinter(signal?: AbortSignal) {
  return apiJson<PrinterState>("printer", { signal });
}

export function stopPrinter() {
  return apiJson<StopDispatched>("printer/emergency-stop", { method: "POST" });
}

/** Homes all axes. The answer comes when the move is done, which can take a minute. */
export function homePrinter() {
  return apiJson<Homed>("printer/home", { method: "POST" });
}

export function sessionDownloadUrl(id: string): string {
  return apiUrl(`sessions/${encodeURIComponent(id)}/download`).toString();
}
