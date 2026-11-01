import type { Axis } from '@/lib/generated/Axis'
import type { SessionDetail } from '@/lib/generated/SessionDetail'
import type { SessionList } from '@/lib/generated/SessionList'
import type { StatusResponse } from '@/lib/generated/StatusResponse'
import type { Summary } from '@/lib/generated/Summary'
import type { Usage } from '@/lib/generated/Usage'

import { ApiClientError, apiFetch, apiJson, apiUrl } from './client'

/** The list filters the app offers. */
export interface SessionFilters {
  chip?: string
  printer?: string
  pinned?: boolean
}

export function listSessions(filters: SessionFilters = {}, signal?: AbortSignal) {
  const url = apiUrl('sessions')
  if (filters.chip) url.searchParams.set('chip', filters.chip)
  if (filters.printer) url.searchParams.set('printer', filters.printer)
  if (filters.pinned !== undefined) url.searchParams.set('pinned', String(filters.pinned))
  return apiJson<SessionList>(url, { signal })
}

export function getSession(id: string, signal?: AbortSignal) {
  return apiJson<SessionDetail>(`sessions/${encodeURIComponent(id)}`, { signal })
}

export function getStorage(signal?: AbortSignal) {
  return apiJson<Usage>('storage', { signal })
}

/** How long the status request may take before the app treats the daemon as not answering. */
export const STATUS_DEADLINE_MS = 4000

/**
 * The daemon's status. A request that never settles would keep the previous healthy answer on
 * screen, because a query reuses a pending fetch, so it fails after [`STATUS_DEADLINE_MS`].
 */
export async function getStatus(signal?: AbortSignal): Promise<StatusResponse> {
  const controller = new AbortController()
  signal?.addEventListener('abort', () => controller.abort())
  let timer: ReturnType<typeof setTimeout> | undefined
  const deadline = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      controller.abort()
      reject(new ApiClientError('The daemon did not answer in time', null))
    }, STATUS_DEADLINE_MS)
  })
  try {
    return await Promise.race([
      apiJson<StatusResponse>('status', { signal: controller.signal }),
      deadline,
    ])
  } finally {
    clearTimeout(timer)
  }
}

export function pinSession(id: string, pinned: boolean) {
  return apiJson<Summary>(`sessions/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ pinned }),
  })
}

export function deleteSession(id: string) {
  return apiJson<void>(`sessions/${encodeURIComponent(id)}`, { method: 'DELETE' })
}

/** What the import form collects. Empty `axis` and `point` are left to the daemon. */
export interface ImportRequest {
  file: File
  chip: string
  axis?: Axis | ''
  point?: string
}

export function importCsv({ file, chip, axis, point }: ImportRequest) {
  const url = apiUrl('sessions')
  url.searchParams.set('chip', chip)
  if (axis) url.searchParams.set('axis', axis.toLowerCase())
  if (point) url.searchParams.set('point', point)
  url.searchParams.set('file_name', file.name)
  return apiJson<Summary>(url, { method: 'POST', body: file })
}

/** The stored, compressed sample file of one recording, exactly as the daemon holds it. */
export async function fetchSample(
  id: string,
  chip: number,
  segment: number,
  signal?: AbortSignal,
): Promise<ArrayBuffer> {
  const response = await apiFetch(
    `sessions/${encodeURIComponent(id)}/samples/${chip}/${segment}`,
    { signal },
  )
  return response.arrayBuffer()
}
