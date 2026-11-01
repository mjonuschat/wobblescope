import { formatHz } from '@/lib/format'
import type { Recommendation } from '@/lib/generated/Recommendation'

import { CopyButton } from './CopyButton'

interface Props {
  title: string
  recommendation: Recommendation
  /** The `[input_shaper]` lines `core` produced for this recommendation. */
  snippet: string | null
}

export function RecommendationCard({ title, recommendation, snippet }: Props) {
  const { best } = recommendation
  return (
    <section className="space-y-3 rounded-lg border p-4" aria-label={title}>
      <h2 className="font-medium">{title}</h2>
      <p className="text-lg">
        {best.name} at {formatHz(best.freq)} Hz
      </p>
      <dl className="grid grid-cols-2 gap-x-6 gap-y-1 text-sm sm:grid-cols-4">
        <div>
          <dt className="text-muted-foreground">Remaining vibrations</dt>
          <dd>{(best.vibrs * 100).toFixed(1)}%</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">Smoothing</dt>
          <dd>{best.smoothing.toFixed(3)} mm</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">Max acceleration</dt>
          <dd>{Math.round(best.max_accel)} mm/s²</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">Score</dt>
          <dd>{best.score.toFixed(3)}</dd>
        </div>
      </dl>
      <p className="text-xs text-muted-foreground">
        Max acceleration is a smoothing-based recommendation, not a machine limit.
      </p>
      {snippet && (
        <div className="space-y-2">
          <pre className="overflow-x-auto rounded bg-muted p-3 text-sm select-text">{snippet}</pre>
          <CopyButton text={snippet} />
        </div>
      )}
    </section>
  )
}
