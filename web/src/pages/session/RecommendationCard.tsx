import type { ReactNode } from "react";

import { Field, Fields } from "@/components/Fields";
import { Panel } from "@/components/Panel";
import { pointLabel } from "@/components/charts/options";
import { formatAccel, formatHz } from "@/lib/format";
import type { CalibrationResult } from "@/lib/generated/CalibrationResult";
import type { DatasetVibration } from "@/lib/generated/DatasetVibration";
import type { ProbePoint } from "@/lib/generated/ProbePoint";
import type { Recommendation } from "@/lib/generated/Recommendation";

import { CopyButton } from "./CopyButton";

const MAX_ACCEL_HINT =
  "Above this acceleration the shaper rounds corners more than you may want. It is a suggestion, not a limit of your printer.";

const SOURCE_TEXT = { Estimated: "Estimated", Fallback: "Fell back to 0.1", Fixed: "Set" } as const;

interface Props {
  title: string;
  recommendation: Recommendation;
  /** The `[input_shaper]` lines `core` produced for this recommendation. */
  snippet: string | null;
  /** The same for the "for performance" shaper, when the recommendation has one. */
  performanceSnippet?: string | null;
  datasetVibrations?: DatasetVibration[];
  probePoints?: ProbePoint[];
}

interface ShaperProps {
  result: CalibrationResult;
  snippet: string | null;
  /** Names the block when there are two. The copy button then sits beside it. */
  heading?: string;
  children?: ReactNode;
}

function Shaper({ result, snippet, heading, children }: ShaperProps) {
  const copy = snippet ? <CopyButton text={snippet} /> : null;
  return (
    <div className="space-y-3">
      {heading && (
        <div className="flex items-center justify-between gap-2">
          <h3 className="text-xs uppercase tracking-wide text-muted-foreground">{heading}</h3>
          {copy}
        </div>
      )}
      <p className="text-xl font-semibold">
        {result.name} at {formatHz(result.freq)} Hz
      </p>
      <Fields className="sm:grid-cols-3 lg:grid-cols-3">
        <Field label="Remaining vibrations">{(result.vibrs * 100).toFixed(1)}%</Field>
        <Field label="Smoothing">{result.smoothing.toFixed(3)} mm</Field>
        <Field label="Max acceleration" hint={MAX_ACCEL_HINT}>
          {formatAccel(result.max_accel)}
        </Field>
      </Fields>
      {children}
      {snippet && (
        <pre className="overflow-x-auto rounded-lg bg-muted p-3 font-mono text-sm select-text">{snippet}</pre>
      )}
    </div>
  );
}

export function RecommendationCard({
  title,
  recommendation,
  snippet,
  performanceSnippet = null,
  datasetVibrations = [],
  probePoints = [],
}: Props) {
  const { best, for_performance: performance, settings } = recommendation;
  const largest = Math.max(0, ...datasetVibrations.map((v) => v.vibrs));
  const multiplePoints = new Set(datasetVibrations.map((v) => v.point)).size > 1;
  const perChip = datasetVibrations.length > 1 && (
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
          </li>
        ))}
      </ul>
    </div>
  );
  const body = (
    <>
      <Shaper result={best} snippet={snippet} heading={performance ? "For low vibrations" : undefined}>
        {perChip}
      </Shaper>
      {performance && <Shaper result={performance} snippet={performanceSnippet} heading="For performance" />}
      <Fields className="sm:grid-cols-3 lg:grid-cols-3">
        <Field label="Damping ratio">
          {settings.damping_ratio.toFixed(3)} ({SOURCE_TEXT[settings.damping_source]})
        </Field>
        {settings.resonance_freq !== null && <Field label="Resonance">{formatHz(settings.resonance_freq)} Hz</Field>}
      </Fields>
    </>
  );
  return (
    <Panel title={title} action={!performance && snippet ? <CopyButton text={snippet} /> : null}>
      {body}
    </Panel>
  );
}
