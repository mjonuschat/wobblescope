import { formatAccel, formatHz } from "@/lib/format";
import type { DatasetVibration } from "@/lib/generated/DatasetVibration";
import type { ProbePoint } from "@/lib/generated/ProbePoint";
import type { Recommendation } from "@/lib/generated/Recommendation";
import { pointLabel } from "@/components/charts/options";

import { CopyButton } from "./CopyButton";

interface Props {
  title: string;
  recommendation: Recommendation;
  /** The `[input_shaper]` lines `core` produced for this recommendation. */
  snippet: string | null;
  datasetVibrations?: DatasetVibration[];
  probePoints?: ProbePoint[];
}

export function RecommendationCard({
  title,
  recommendation,
  snippet,
  datasetVibrations = [],
  probePoints = [],
}: Props) {
  const { best } = recommendation;
  const largest = Math.max(0, ...datasetVibrations.map((v) => v.vibrs));
  const multiplePoints = new Set(datasetVibrations.map((v) => v.point)).size > 1;
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
          <dd>{formatAccel(best.max_accel)}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">Score</dt>
          <dd>{best.score.toFixed(3)}</dd>
        </div>
      </dl>
      {datasetVibrations.length > 1 && (
        <div className="space-y-1 text-sm">
          <h3 className="text-muted-foreground">Remaining vibrations by chip</h3>
          <ul>
            {datasetVibrations.map((v) => (
              <li
                key={`${v.chip}-${v.point}`}
                data-limiting={v.vibrs === largest}
                className={v.vibrs === largest ? "font-medium" : undefined}
              >
                {v.chip}
                {multiplePoints && ` ${pointLabel(v.point, probePoints)}`}: {(v.vibrs * 100).toFixed(1)}%
                {v.vibrs === largest && " (sets the worst case)"}
              </li>
            ))}
          </ul>
        </div>
      )}
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
  );
}
