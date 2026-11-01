import { pointLabel } from '@/components/charts/options'
import type { Analysis } from '@/lib/generated/Analysis'
import type { ProbePoint } from '@/lib/generated/ProbePoint'

export function EligibilityPanel({
  analysis,
  probePoints,
}: {
  analysis: Analysis
  probePoints: ProbePoint[]
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
