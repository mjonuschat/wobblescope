import type { Axis } from '@/lib/generated/Axis'
import type { SessionDetail } from '@/lib/generated/SessionDetail'
import type { SessionList } from '@/lib/generated/SessionList'
import type { Summary } from '@/lib/generated/Summary'
import type { Usage } from '@/lib/generated/Usage'

import { apiFetch, apiJson, apiUrl } from './client'

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
