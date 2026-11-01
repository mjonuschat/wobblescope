import type { SessionFilters as Filters } from '@/lib/api/endpoints'
import type { Summary } from '@/lib/generated/Summary'
import { Label } from '@/components/ui/label'

const selectClass =
  'h-8 rounded-lg border border-input bg-transparent px-2 text-sm outline-none focus-visible:border-ring'

interface Props {
  filters: Filters
  onChange: (filters: Filters) => void
  /** The unfiltered list, which supplies the chip and printer choices. */
  all: Summary[]
}

export function SessionFilters({ filters, onChange, all }: Props) {
  const chips = [...new Set(all.flatMap((s) => s.chips))].sort()
  const printers = [...new Set(all.flatMap((s) => (s.printer ? [s.printer] : [])))].sort()
  return (
    <div className="flex flex-wrap gap-4">
      <div className="flex flex-col gap-1">
        <Label htmlFor="filter-chip">Chip</Label>
        <select
          id="filter-chip"
          className={selectClass}
          value={filters.chip ?? ''}
          onChange={(e) => onChange({ ...filters, chip: e.target.value || undefined })}
        >
          <option value="">All chips</option>
          {chips.map((c) => (
            <option key={c}>{c}</option>
          ))}
        </select>
      </div>
      <div className="flex flex-col gap-1">
        <Label htmlFor="filter-printer">Printer</Label>
        <select
          id="filter-printer"
          className={selectClass}
          value={filters.printer ?? ''}
          onChange={(e) => onChange({ ...filters, printer: e.target.value || undefined })}
        >
          <option value="">All printers</option>
          {printers.map((p) => (
            <option key={p}>{p}</option>
          ))}
        </select>
      </div>
      <div className="flex flex-col gap-1">
        <Label htmlFor="filter-pinned">Pinned</Label>
        <select
          id="filter-pinned"
          className={selectClass}
          value={filters.pinned === undefined ? '' : String(filters.pinned)}
          onChange={(e) =>
            onChange({ ...filters, pinned: e.target.value === '' ? undefined : e.target.value === 'true' })
          }
        >
          <option value="">All sessions</option>
          <option value="true">Pinned only</option>
          <option value="false">Unpinned only</option>
        </select>
      </div>
    </div>
  )
}
