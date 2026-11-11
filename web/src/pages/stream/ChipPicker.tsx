import type { ReactNode } from 'react'

import { Badge } from '@/components/ui/badge'
import type { Availability } from '@/lib/generated/Availability'
import type { Chip } from '@/lib/generated/Chip'

import type { Axis3 } from './axes'

const AXIS_CHOICES: { axis: Axis3; label: string }[] = [
  { axis: 'x', label: 'X' },
  { axis: 'y', label: 'Y' },
  { axis: 'z', label: 'Z' },
  { axis: 'sum', label: 'Total' },
]

interface Props {
  chips: Chip[]
  selected: string[]
  maxChips: number
  axes: Record<string, Axis3[]>
  onToggle: (section: string) => void
  onAxes: (section: string, axes: Axis3[]) => void
  badge: (section: string) => ReactNode
}

function check(availability: Availability): string {
  switch (availability.state) {
    case 'unchecked':
      return 'unchecked'
    case 'available':
      return `available · ${Math.round(availability.observed_rate_hz)} Hz`
    case 'unavailable':
      return `unavailable: ${availability.reason}`
  }
}

/** The chips to stream, at most `maxChips` at once, each with its own axis toggles. */
export function ChipPicker({ chips, selected, maxChips, axes, onToggle, onAxes, badge }: Props) {
  const full = selected.length >= maxChips
  return (
    <ul className="space-y-3">
      {chips.map((chip) => {
        const on = selected.includes(chip.section)
        const blocked = !on && full
        const chosen = axes[chip.section] ?? ['sum']
        return (
          <li key={chip.section} className="space-y-2 rounded-md border border-neutral-800 p-3">
            <div className="flex flex-wrap items-center gap-3">
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  aria-label={`Stream ${chip.section}`}
                  checked={on}
                  disabled={blocked}
                  onChange={() => onToggle(chip.section)}
                />
                {chip.section}
              </label>
              <Badge variant="outline">{chip.chip_type}</Badge>
              <span className="text-sm text-neutral-400">
                {chip.nominal_rate_hz === null ? 'rate unknown' : `${chip.nominal_rate_hz} Hz`}
              </span>
              <span className="text-sm text-neutral-400">{check(chip.availability)}</span>
              {blocked && <span className="text-sm text-amber-400">at most {maxChips} chips at once</span>}
            </div>
            {on && (
              <div className="space-y-2">
                <div role="group" aria-label={`Axes of ${chip.section}`} className="flex gap-2">
                  {AXIS_CHOICES.map(({ axis, label }) => {
                    const pressed = chosen.includes(axis)
                    return (
                      <button
                        key={axis}
                        type="button"
                        aria-pressed={pressed}
                        aria-label={`${label} of ${chip.section}`}
                        className={`rounded-md border px-2 py-0.5 text-xs ${pressed ? 'border-neutral-300 bg-neutral-700' : 'border-neutral-700'}`}
                        onClick={() => {
                          const next = pressed ? chosen.filter((a) => a !== axis) : [...chosen, axis]
                          if (next.length > 0) onAxes(chip.section, next)
                        }}
                      >
                        {label}
                      </button>
                    )
                  })}
                </div>
                {badge(chip.section)}
              </div>
            )}
          </li>
        )
      })}
    </ul>
  )
}
