import { pointLabel } from '@/components/charts/options'
import type { Analysis } from '@/lib/generated/Analysis'
import type { ProbePoint } from '@/lib/generated/ProbePoint'

/** The chips with a dataset that health or coverage excludes, or that an override includes. */
function overridable(analysis: Analysis) {
  const chips = new Map<string, { overridden: boolean; reasons: string[] }>()
  for (const d of analysis.eligibility.decisions) {
    if (d.decision === 'Included') continue
    const entry = chips.get(d.chip) ?? { overridden: false, reasons: [] }
    entry.overridden ||= d.decision === 'Overridden'
    for (const reason of d.reasons) if (!entry.reasons.includes(reason)) entry.reasons.push(reason)
    chips.set(d.chip, entry)
  }
  return chips
}

export function EligibilityPanel({
  analysis,
  probePoints,
  overrides,
  onOverridesChange,
}: {
  analysis: Analysis
  probePoints: ProbePoint[]
  overrides: string[]
  onOverridesChange: (overrides: string[]) => void
}) {
  const { decisions } = analysis.eligibility
  if (decisions.length === 0 && analysis.failures.length === 0) return null
  return (
    <section className="space-y-2 rounded-lg border p-4" aria-label="Capture health and eligibility">
      <h2 className="font-medium">Capture health and eligibility</h2>
      <ul className="space-y-1 text-sm">
        {decisions.map((d) => (
          <li key={`${d.chip}-${d.point}`}>
            <span className="font-medium">
              {d.chip} at {pointLabel(d.point, probePoints)}
            </span>
            : {d.decision === 'Excluded' ? 'excluded' : d.decision === 'Overridden' ? 'included by override' : 'included'}
            {d.reasons.length > 0 && (
              <span className={d.decision === 'Excluded' ? 'text-destructive' : 'text-amber-400'}>
                {' '}
                ({d.reasons.join('; ')})
              </span>
            )}
          </li>
        ))}
      </ul>
      {[...overridable(analysis)].map(([chip, { overridden, reasons }]) => (
        <label key={chip} className="flex items-start gap-2 text-sm">
          <input
            type="checkbox"
            className="mt-1"
            checked={overridden}
            onChange={(e) =>
              onOverridesChange(
                e.target.checked ? [...overrides, chip] : overrides.filter((c) => c !== chip),
              )
            }
          />
          <span>
            Include {chip} anyway
            <span className="text-muted-foreground"> ({reasons.join('; ')})</span>
          </span>
        </label>
      ))}
      {analysis.failures.length > 0 && (
        <ul className="space-y-1 text-sm text-destructive">
          {analysis.failures.map((f) => (
            <li key={`${f.chip}-${f.point}`}>
              {f.chip} at {pointLabel(f.point, probePoints)} could not be processed: {f.error}
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
