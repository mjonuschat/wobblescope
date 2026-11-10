import { useState } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { ApiClientError } from '@/lib/api/client'
import { useStartTest } from '@/lib/api/queries'
import type { Axis } from '@/lib/generated/Axis'
import type { CheckFailure } from '@/lib/generated/CheckFailure'
import type { Mode } from '@/lib/generated/Mode'
import type { PrinterSettings } from '@/lib/generated/PrinterSettings'
import { parseNumber } from '@/lib/parse'

import {
  OVERRIDE_FIELDS,
  type OverrideKey,
  formatDuration,
  segmentSeconds,
} from './planning'

/** The consequence of the emergency stop, shown before a test starts. */
export const STOP_CONSEQUENCE =
  'Emergency stop puts Klipper into shutdown. You will need a FIRMWARE_RESTART afterwards.'

interface PointRow {
  x: string
  y: string
  z: string
}

interface Props {
  settings: PrinterSettings
  /** The chips that will record. */
  chips: string[]
  /** Why a test cannot start now, or `null`. */
  blocked: string | null
  /** Called with the new test's id after the daemon accepted the test. */
  onStarted: (id: string) => void
}

function rowFrom(point: [number, number, number] | null): PointRow {
  return point
    ? { x: String(point[0]), y: String(point[1]), z: String(point[2]) }
    : { x: '', y: '', z: '' }
}

/** The daemon names the field it refuses as a `field: reason` prefix. */
function fieldOf(message: string): { field: string; text: string } | null {
  const match = /^([a-z_]+): (.*)$/s.exec(message)
  return match ? { field: match[1], text: match[2] } : null
}

function FieldError({ name, text }: { name: string; text: string | undefined }) {
  if (!text) return null
  return (
    <p id={`error-${name}`} role="alert" className="text-sm text-destructive">
      {text}
    </p>
  )
}

/** The run form: axes, mode, overrides, points, and the start button. */
export function TestForm({ settings, chips, blocked, onStarted }: Props) {
  const start = useStartTest()
  const [axes, setAxes] = useState<Axis[]>(['X', 'Y'])
  const [chosenMode, setChosenMode] = useState<Mode | null>(null)
  const [texts, setTexts] = useState<Partial<Record<OverrideKey, string>>>({})
  const [points, setPoints] = useState<PointRow[]>(() => [rowFrom(settings.default_point)])
  const mode = chosenMode ?? settings.default_mode

  const overrides: Partial<Record<OverrideKey, number>> = {}
  const typos: Partial<Record<OverrideKey, string>> = {}
  for (const { key } of OVERRIDE_FIELDS) {
    const value = parseNumber(texts[key] ?? '')
    if (value === undefined) continue
    if (Number.isNaN(value)) typos[key] = 'Enter a number.'
    else overrides[key] = value
  }
  const sweepOnly = (key: OverrideKey) =>
    mode === 'Pulse' && (key === 'sweeping_accel' || key === 'sweeping_period')

  const parsedPoints = points.map((p) => [parseNumber(p.x), parseNumber(p.y), parseNumber(p.z)])
  const pointsValid =
    parsedPoints.length > 0 &&
    parsedPoints.every((p) => p.every((v) => v !== undefined && !Number.isNaN(v)))
  const missing = OVERRIDE_FIELDS.filter(
    ({ key }) => settings.settings[key].needed && settings.settings[key].value === null && overrides[key] === undefined,
  )

  const seconds = segmentSeconds(settings, overrides)
  const segmentCount = points.length * axes.length

  let problem = blocked
  if (!problem && chips.length === 0) problem = 'Choose at least one chip.'
  if (!problem && axes.length === 0) problem = 'Choose at least one axis.'
  if (!problem && !pointsValid) problem = 'Give every test point an X, Y and Z.'
  if (!problem && missing.length > 0) {
    problem = `Set ${missing.map((m) => m.label.toLowerCase()).join(', ')}: the printer does not report it.`
  }
  if (!problem && Object.keys(typos).length > 0) problem = 'Fix the settings that are not numbers.'

  const error = start.error
  const body = error instanceof ApiClientError ? error.body : null
  const named = body?.error === 'invalid_request' ? fieldOf(body.message) : null
  const failures: CheckFailure[] = body?.error === 'preflight_failed' ? body.failures : []
  const general =
    error && !named && failures.length === 0
      ? error.message
      : named && !['chips', 'axes', 'points', ...OVERRIDE_FIELDS.map((f) => f.key)].includes(named.field)
        ? `${named.field}: ${named.text}`
        : null
  const errorFor = (field: string) => (named?.field === field ? named.text : undefined)

  function toggleAxis(axis: Axis) {
    setAxes((old) => (old.includes(axis) ? old.filter((a) => a !== axis) : [...old, axis]))
  }

  function submit(event: React.FormEvent) {
    event.preventDefault()
    if (problem || start.isPending) return
    start.mutate(
      {
        chips,
        axes,
        mode,
        overrides: Object.fromEntries(
          Object.entries(overrides).filter(([key]) => !sweepOnly(key as OverrideKey)),
        ),
        points: parsedPoints.map((p) => p as [number, number, number]),
      },
      { onSuccess: (started) => onStarted(started.id) },
    )
  }

  return (
    <form onSubmit={submit} className="space-y-6" noValidate>
      <fieldset className="space-y-2">
        <legend className="text-lg font-semibold">Axes</legend>
        <div className="flex gap-4">
          {(['X', 'Y'] as const).map((axis) => (
            <label key={axis} className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={axes.includes(axis)}
                onChange={() => toggleAxis(axis)}
              />
              {axis}
            </label>
          ))}
        </div>
        <FieldError name="axes" text={errorFor('axes')} />
      </fieldset>

      <fieldset className="space-y-2">
        <legend className="text-lg font-semibold">Mode</legend>
        <div className="flex gap-4">
          {(['Pulse', 'Sweep'] as const).map((m) => (
            <label key={m} className="flex items-center gap-2 text-sm">
              <input
                type="radio"
                name="mode"
                checked={mode === m}
                onChange={() => setChosenMode(m)}
              />
              {m}
            </label>
          ))}
        </div>
        <FieldError name="mode" text={errorFor('mode')} />
      </fieldset>

      <fieldset className="space-y-3">
        <legend className="text-lg font-semibold">Settings</legend>
        <p className="text-sm text-muted-foreground">
          Leave a field empty to use the printer's value, shown in the field.
        </p>
        <div className="grid grid-cols-2 gap-4">
          {OVERRIDE_FIELDS.map(({ key, label, unit }) => {
            const setting = settings.settings[key]
            const required = setting.needed && setting.value === null
            const placeholder =
              setting.value !== null
                ? String(setting.value)
                : required
                  ? 'required'
                  : 'printer default'
            return (
              <div key={key} className="space-y-1">
                <Label htmlFor={`override-${key}`}>
                  {label} ({unit})
                  {required ? ' *' : ''}
                </Label>
                <Input
                  id={`override-${key}`}
                  inputMode="decimal"
                  placeholder={placeholder}
                  value={texts[key] ?? ''}
                  disabled={sweepOnly(key) || !setting.overridable}
                  aria-required={required}
                  aria-invalid={Boolean(typos[key] ?? errorFor(key))}
                  aria-describedby={`error-${key}`}
                  onChange={(e) => setTexts((old) => ({ ...old, [key]: e.target.value }))}
                />
                <FieldError name={key} text={typos[key] ?? errorFor(key)} />
              </div>
            )
          })}
        </div>
      </fieldset>

      <fieldset className="space-y-2">
        <legend className="text-lg font-semibold">Test points</legend>
        {points.map((row, i) => (
          <div key={i} className="flex items-end gap-2">
            {(['x', 'y', 'z'] as const).map((axis) => (
              <div key={axis} className="space-y-1">
                <Label htmlFor={`point-${i}-${axis}`}>
                  Point {i + 1} {axis.toUpperCase()} (mm)
                </Label>
                <Input
                  id={`point-${i}-${axis}`}
                  inputMode="decimal"
                  className="w-28"
                  value={row[axis]}
                  onChange={(e) =>
                    setPoints((old) =>
                      old.map((r, j) => (j === i ? { ...r, [axis]: e.target.value } : r)),
                    )
                  }
                />
              </div>
            ))}
            <Button
              type="button"
              variant="outline"
              disabled={points.length === 1}
              onClick={() => setPoints((old) => old.filter((_, j) => j !== i))}
            >
              Remove point {i + 1}
            </Button>
          </div>
        ))}
        <FieldError name="points" text={errorFor('points')} />
        <Button
          type="button"
          variant="outline"
          onClick={() => setPoints((old) => [...old, rowFrom(settings.default_point)])}
        >
          Add a point
        </Button>
      </fieldset>

      <div className="space-y-2">
        <p className="text-sm">
          {segmentCount} {segmentCount === 1 ? 'segment' : 'segments'}
          {seconds !== null && segmentCount > 0
            ? ` · about ${formatDuration(seconds * segmentCount)}`
            : ''}
        </p>
        <p className="text-sm text-muted-foreground">{STOP_CONSEQUENCE}</p>
        <FieldError name="chips" text={errorFor('chips')} />
        {general && (
          <p role="alert" className="text-sm text-destructive">
            {general}
          </p>
        )}
        {failures.length > 0 && (
          <div role="alert" className="space-y-1 text-sm text-destructive">
            <p>The test did not start:</p>
            <ul className="list-disc pl-5">
              {failures.map((f) => (
                <li key={f.check}>{f.message}</li>
              ))}
            </ul>
          </div>
        )}
        {problem && <p className="text-sm text-muted-foreground">{problem}</p>}
        <Button type="submit" disabled={problem !== null || start.isPending}>
          {start.isPending ? 'Checking printer…' : 'Start test'}
        </Button>
      </div>
    </form>
  )
}
