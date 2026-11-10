import type { Overrides } from '@/lib/generated/Overrides'
import type { PrinterSettings } from '@/lib/generated/PrinterSettings'

/** The seven settings a test may override, in the order the form shows them. */
export const OVERRIDE_FIELDS = [
  { key: 'freq_start', label: 'Start frequency', unit: 'Hz' },
  { key: 'freq_end', label: 'End frequency', unit: 'Hz' },
  { key: 'hz_per_sec', label: 'Hz per second', unit: 'Hz/s' },
  { key: 'accel_per_hz', label: 'Acceleration per Hz', unit: 'mm/s² per Hz' },
  { key: 'sweeping_accel', label: 'Sweeping acceleration', unit: 'mm/s²' },
  { key: 'sweeping_period', label: 'Sweeping period', unit: 's' },
  { key: 'move_speed', label: 'Move speed', unit: 'mm/s' },
] as const satisfies readonly { key: keyof Overrides; label: string; unit: string }[]

export type OverrideKey = (typeof OVERRIDE_FIELDS)[number]['key']

/** A number typed into a field, `undefined` when it is empty and `NaN` when it is not a number. */
export function parseNumber(text: string): number | undefined {
  const trimmed = text.trim()
  if (trimmed === '') return undefined
  const value = Number(trimmed)
  return Number.isFinite(value) ? value : Number.NaN
}

/** The value a test runs with: the override, else the printer's value. */
export function effective(
  settings: PrinterSettings,
  overrides: Partial<Record<OverrideKey, number>>,
  key: OverrideKey,
): number | null {
  return overrides[key] ?? settings.settings[key].value
}

/** Expected seconds of one segment, or `null` when a value it needs is not known. */
export function segmentSeconds(
  settings: PrinterSettings,
  overrides: Partial<Record<OverrideKey, number>>,
): number | null {
  const start = effective(settings, overrides, 'freq_start')
  const end = effective(settings, overrides, 'freq_end')
  const rate = effective(settings, overrides, 'hz_per_sec')
  if (start === null || end === null || rate === null || rate <= 0) return null
  return (end - start) / rate + settings.segment_overhead_seconds
}

/** A duration as minutes and seconds, such as `7 min` or `45 s`. */
export function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.round(seconds))
  if (total < 60) return `${total} s`
  const minutes = Math.floor(total / 60)
  const rest = total % 60
  return rest === 0 ? `${minutes} min` : `${minutes} min ${rest} s`
}

/** A point as `x, y, z`. */
export function formatPoint(point: readonly number[]): string {
  return point.map((v) => String(Math.round(v * 1000) / 1000)).join(', ')
}
