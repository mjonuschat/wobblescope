import { useNavigate } from '@tanstack/react-router'
import { useState } from 'react'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { ApiClientError } from '@/lib/api/client'
import { useChips, useImportCsv } from '@/lib/api/queries'
import { formatBytes } from '@/lib/format'
import type { Axis } from '@/lib/generated/Axis'

/** The select entry that reveals the typed field. */
const OTHER = '__other__'

/** The daemon refuses a larger body, so the browser does not send one. */
export const MAX_IMPORT_BYTES = 64 * 1024 * 1024

function describe(error: Error): string {
  if (!(error instanceof ApiClientError) || !error.body) return error.message
  const body = error.body
  switch (body.error) {
    case 'busy':
      return `Another import is running. Try again in a moment. ${body.message}`
    case 'cap_exceeded':
      return `${body.message} The import needs ${formatBytes(body.need)} and the cap is ${formatBytes(body.cap)}.`
    case 'insufficient_disk':
      return `${body.message} The import needs ${formatBytes(body.need)} and ${formatBytes(body.available)} is free.`
    default:
      return body.message
  }
}

interface Props {
  open: boolean
  onOpenChange: (open: boolean) => void
}

/** The dialog's content is only mounted while it is open, so closing it clears the form. */
export function ImportDialog({ open, onOpenChange }: Props) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <ImportForm onDone={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  )
}

function ImportForm({ onDone }: { onDone: () => void }) {
  const navigate = useNavigate()
  const importCsv = useImportCsv()
  const [file, setFile] = useState<File | null>(null)
  const chips = useChips()
  const known = chips.data?.known ? chips.data.chips.map((c) => c.section) : []
  // With known chips the user picks one, or "Other…" to type a name. Without them only the
  // text field shows, as before.
  const [choice, setChoice] = useState('')
  const [typed, setTyped] = useState('')
  const typing = known.length === 0 || choice === OTHER
  const chip = typing ? typed : choice
  const [axis, setAxis] = useState<Axis | ''>('')
  const [point, setPoint] = useState('')
  const [problem, setProblem] = useState<{ field: 'file' | 'chip'; text: string } | null>(null)

  function submit(event: React.FormEvent) {
    event.preventDefault()
    if (!chip.trim()) return setProblem({ field: 'chip', text: 'Enter the chip name.' })
    if (!file) return setProblem({ field: 'file', text: 'Choose a CSV file.' })
    if (file.size > MAX_IMPORT_BYTES) {
      return setProblem({
        field: 'file',
        text: `This file is ${formatBytes(file.size)}. The limit is ${formatBytes(MAX_IMPORT_BYTES)}.`,
      })
    }
    setProblem(null)
    importCsv.mutate(
      { file, chip: chip.trim(), axis, point: point.trim() },
      {
        onSuccess: (summary) => {
          onDone()
          void navigate({ to: '/sessions/$id', params: { id: summary.id } })
        },
      },
    )
  }

  return (
    <form onSubmit={submit} className="space-y-4" noValidate>
      <DialogHeader>
        <DialogTitle>Import a Klipper CSV</DialogTitle>
        <DialogDescription>
          A raw accelerometer file from <code>TEST_RESONANCES OUTPUT=raw_data</code>.
        </DialogDescription>
      </DialogHeader>
      <div className="space-y-1">
        <Label htmlFor="import-file">CSV file</Label>
        <Input
          id="import-file"
          type="file"
          accept=".csv,text/csv"
          aria-invalid={problem?.field === 'file'}
          onChange={(e) => setFile(e.target.files?.[0] ?? null)}
        />
      </div>
      {known.length > 0 && (
        <div className="space-y-1">
          <Label htmlFor="import-chip-select">Chip</Label>
          <select
            id="import-chip-select"
            className="h-8 w-full rounded-lg border border-input bg-transparent px-2 text-sm"
            value={choice}
            aria-invalid={problem?.field === 'chip' && !typing}
            onChange={(e) => setChoice(e.target.value)}
          >
            <option value="">Choose a chip</option>
            {known.map((section) => (
              <option key={section} value={section}>
                {section}
              </option>
            ))}
            <option value={OTHER}>Other…</option>
          </select>
        </div>
      )}
      {typing && (
        <div className="space-y-1">
          <Label htmlFor="import-chip">Chip name</Label>
          <Input
            id="import-chip"
            placeholder="adxl345"
            value={typed}
            aria-invalid={problem?.field === 'chip'}
            onChange={(e) => setTyped(e.target.value)}
          />
        </div>
      )}
      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-1">
          <Label htmlFor="import-axis">Axis</Label>
          <select
            id="import-axis"
            className="h-8 w-full rounded-lg border border-input bg-transparent px-2 text-sm"
            value={axis}
            onChange={(e) => setAxis(e.target.value as Axis | '')}
          >
            <option value="">From the file name</option>
            <option value="X">X</option>
            <option value="Y">Y</option>
          </select>
        </div>
        <div className="space-y-1">
          <Label htmlFor="import-point">Point (x,y,z in mm)</Label>
          <Input
            id="import-point"
            placeholder="From the file name"
            value={point}
            onChange={(e) => setPoint(e.target.value)}
          />
        </div>
      </div>
      {problem && (
        <p role="alert" className="text-sm text-destructive">
          {problem.text}
        </p>
      )}
      {importCsv.isError && (
        <p role="alert" className="text-sm text-destructive">
          {describe(importCsv.error)}
        </p>
      )}
      <DialogFooter>
        <Button type="submit" disabled={importCsv.isPending}>
          {importCsv.isPending ? 'Importing…' : 'Import'}
        </Button>
      </DialogFooter>
    </form>
  )
}
