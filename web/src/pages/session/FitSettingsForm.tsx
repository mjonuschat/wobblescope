import { useState } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import type { FitParams } from '@/lib/generated/FitParams'

import { type FitDraft, defaultDraft, defaultFitParams, draftToParams } from './fit'

const FIELDS: { key: keyof FitDraft; label: string; hint: string }[] = [
  { key: 'scv', label: 'Square corner velocity (mm/s)', hint: 'Used to estimate smoothing.' },
  { key: 'damping_ratio', label: 'Damping ratio', hint: 'Empty uses 0.1.' },
  { key: 'max_smoothing', label: 'Smoothing limit (mm)', hint: 'Empty means no limit.' },
  { key: 'max_vibrations', label: 'Vibration limit (fraction)', hint: 'Empty uses the default.' },
]

/** Fit settings for this view only; nothing here is saved. */
export function FitSettingsForm({ onApply }: { onApply: (params: FitParams) => void }) {
  const [draft, setDraft] = useState<FitDraft>(defaultDraft)
  const result = draftToParams(draft)
  const invalid = 'invalid' in result ? result.invalid : null

  return (
    <form
      className="space-y-3 rounded-lg border p-4"
      onSubmit={(event) => {
        event.preventDefault()
        if ('params' in result) onApply(result.params)
      }}
    >
      <h2 className="font-medium">Fit settings</h2>
      <div className="grid gap-4 sm:grid-cols-4">
        {FIELDS.map(({ key, label, hint }) => (
          <div key={key} className="space-y-1">
            <Label htmlFor={`fit-${key}`}>{label}</Label>
            <Input
              id={`fit-${key}`}
              inputMode="decimal"
              value={draft[key]}
              aria-invalid={invalid === key}
              onChange={(e) => setDraft({ ...draft, [key]: e.target.value })}
            />
            <p className="text-xs text-muted-foreground">{hint}</p>
          </div>
        ))}
      </div>
      {invalid && <p className="text-sm text-destructive">Enter a number, or leave the field empty.</p>}
      <div className="flex gap-2">
        <Button type="submit" disabled={invalid !== null}>
          Apply
        </Button>
        <Button
          type="button"
          variant="outline"
          onClick={() => {
            setDraft(defaultDraft)
            onApply(defaultFitParams)
          }}
        >
          Reset
        </Button>
      </div>
    </form>
  )
}
