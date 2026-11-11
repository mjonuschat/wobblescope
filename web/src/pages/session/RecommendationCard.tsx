import { Field, Fields } from "@/components/Fields";
import { Panel } from "@/components/Panel";
import { pointLabel } from "@/components/charts/options";
import { formatAccel, formatHz } from "@/lib/format";
import type { DatasetVibration } from "@/lib/generated/DatasetVibration";
import type { ProbePoint } from "@/lib/generated/ProbePoint";
import type { Recommendation } from "@/lib/generated/Recommendation";

import { CopyButton } from "./CopyButton";

const MAX_ACCEL_HINT =
  "Above this acceleration the shaper rounds corners more than you may want. It is a suggestion, not a limit of your printer.";

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
  const copy = snippet ? <CopyButton text={snippet} /> : null;
  const body = (
    <>
      <p className="text-xl font-semibold">
        {best.name} at {formatHz(best.freq)} Hz
      </p>
      <Fields className="sm:grid-cols-3 lg:grid-cols-3">
        <Field label="Remaining vibrations">{(best.vibrs * 100).toFixed(1)}%</Field>
        <Field label="Smoothing">{best.smoothing.toFixed(3)} mm</Field>
        <Field label="Max acceleration" hint={MAX_ACCEL_HINT}>
          {formatAccel(best.max_accel)}
        </Field>
      </Fields>
      {datasetVibrations.length > 1 && (
        <div className="space-y-1 text-sm">
          <h3 className="text-xs uppercase tracking-wide text-muted-foreground">Remaining vibrations by chip</h3>
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
      {snippet && (
        <pre className="overflow-x-auto rounded-lg bg-muted p-3 font-mono text-sm select-text">{snippet}</pre>
      )}
    </>
  );
  return (
    <Panel title={title} action={copy}>
      {body}
    </Panel>
  );
}
