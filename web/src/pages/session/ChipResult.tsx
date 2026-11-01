import { EChart } from '@/components/charts/EChart'
import { psdOption } from '@/components/charts/options'
import { Badge } from '@/components/ui/badge'
import { formatHz } from '@/lib/format'
import type { ChipAnalysis } from '@/lib/generated/ChipAnalysis'
import type { DatasetDecision } from '@/lib/generated/DatasetDecision'
import type { ProbePoint } from '@/lib/generated/ProbePoint'

import { RecommendationCard } from './RecommendationCard'
import { ShaperTable } from './ShaperTable'

interface Props {
  chip: ChipAnalysis
  snippet: string | null
  probePoints: ProbePoint[]
  maxFreq: number
  decisions: DatasetDecision[]
}

export function ChipResult({ chip, snippet, probePoints, maxFreq, decisions }: Props) {
  const rec = chip.recommendation
  const reasons = decisions
    .filter((d) => d.chip === chip.chip && d.decision === 'Excluded')
    .flatMap((d) => d.reasons)
  const option = psdOption({ chip, recommendation: rec, probePoints, maxFreq })
  return (
    <section className="space-y-3 rounded-lg border p-4" aria-label={`Chip ${chip.chip}`}>
      <div className="flex items-center gap-2">
        <h2 className="font-medium">{chip.chip}</h2>
        <Badge variant={chip.health === 'Degraded' ? 'destructive' : 'secondary'}>
          {chip.health}
        </Badge>
      </div>
      <EChart option={option} label={`PSD of ${chip.chip}${rec ? `, recommended ${rec.best.name} at ${formatHz(rec.best.freq)} Hz` : ''}`} />
      {rec ? (
        <>
          <ShaperTable recommendation={rec} />
          <RecommendationCard title={`Recommendation for ${chip.chip}`} recommendation={rec} snippet={snippet} />
        </>
      ) : (
        <p className="text-sm">
          No recommendation for this chip{reasons.length > 0 && `: ${[...new Set(reasons)].join('; ')}`}.
        </p>
      )}
    </section>
  )
}
