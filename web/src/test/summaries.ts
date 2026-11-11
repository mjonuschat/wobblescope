import type { Summary } from '@/lib/generated/Summary'
import type { Usage } from '@/lib/generated/Usage'

export function summary(id: string, over: Partial<Summary> = {}): Summary {
  return {
    id,
    created_at: '2026-10-31T12:00:00Z',
    created_at_unix: 1793448000,
    source: 'Capture',
    outcome: 'Complete',
    mode: 'Sweep',
    printer: 'voron',
    chips: ['adxl345'],
    size_bytes: 1536,
    pinned: false,
    ...over,
  }
}

export const usage: Usage = {
  used_bytes: 1.5 * 1024 ** 3,
  reserved_bytes: 100 * 1024 ** 2,
  pinned_bytes: 500 * 1024 ** 2,
  cap_bytes: 2 * 1024 ** 3,
  free_bytes: 10 * 1024 ** 3,
  session_count: 12,
  result_count: 0,
  result_bytes: 0,
}
