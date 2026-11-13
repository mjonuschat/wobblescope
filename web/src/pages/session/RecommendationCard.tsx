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

/** What to tell about the damping ratio when the fit looked at a resonance. A fixed ratio needs no note. */
function dampingHintFor(settings: Recommendation["settings"]): string | undefined {
  if (settings.resonance_freq === null) return undefined;
  const at = `${formatHz(settings.resonance_freq)} Hz`;
  if (settings.damping_source === "Estimated") return `Estimated from the strongest resonance, at ${at}.`;
  if (settings.damping_source === "Fallback") {
    return `The strongest resonance, at ${at}, gave no estimate, so 0.1 was used.`;
  }
  return undefined;
}

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
  /** Names the block when there are two. A named block is a sub-card, sized for half the card. */
  heading?: string;
  /** The fit's damping ratio, shown with the shaper's numbers. */
  damping: ReactNode;
  /** Where the ratio came from, when there is something to say. */
  dampingHint?: string;
  /** What each chip leaves behind, shown under the remaining vibrations. */
  byChip?: ReactNode;
}

function Shaper({ result, snippet, heading, damping, dampingHint, byChip }: ShaperProps) {
  return (
    <section aria-label={heading} className={heading ? "min-w-0 space-y-3 rounded-lg border p-4" : "space-y-3"}>
      {heading && <h3 className="text-xs uppercase tracking-wide text-muted-foreground">{heading}</h3>}
      <p className="text-xl font-semibold">
        {result.name} at {formatHz(result.freq)} Hz
      </p>
      <Fields className={heading ? "sm:grid-cols-2 lg:grid-cols-2" : "sm:grid-cols-4 lg:grid-cols-4"}>
        <Field label="Remaining vibrations">
          {(result.vibrs * 100).toFixed(1)}%{byChip}
        </Field>
        <Field label="Smoothing">{result.smoothing.toFixed(3)} mm</Field>
        <Field label="Max acceleration" hint={MAX_ACCEL_HINT}>
          {formatAccel(result.max_accel)}
        </Field>
        <Field label="Damping ratio" hint={dampingHint}>
          {damping}
        </Field>
      </Fields>
      {snippet && (
        <div className="relative">
          <pre className="overflow-x-auto rounded-lg bg-muted p-3 pr-14 font-mono text-sm select-text">{snippet}</pre>
          <div className="absolute top-2 right-2 rounded-lg bg-muted">
            <CopyButton text={snippet} />
          </div>
        </div>
      )}
    </section>
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
  const multiplePoints = new Set(datasetVibrations.map((v) => v.point)).size > 1;
  /** What each chip leaves behind with a shaper. */
  const byChip = (figures: DatasetVibration[]) => {
    const largest = Math.max(0, ...figures.map((v) => v.vibrs));
    return (
      figures.length > 1 && (
        <ul className="mt-1 flex flex-wrap font-sans text-xs text-muted-foreground [&>li:not(:last-child)]:after:mx-1.5 [&>li:not(:last-child)]:after:content-['·']">
          {figures.map((v) => (
            <li
              key={`${v.chip}-${v.point}`}
              data-limiting={v.vibrs === largest}
              className={v.vibrs === largest ? "font-medium text-foreground" : undefined}
            >
              {v.chip}
              {multiplePoints && ` ${pointLabel(v.point, probePoints)}`}: {(v.vibrs * 100).toFixed(1)}%
            </li>
          ))}
        </ul>
      )
    );
  };
  const damping = `${settings.damping_ratio.toFixed(3)} (${SOURCE_TEXT[settings.damping_source]})`;
  const dampingHint = dampingHintFor(settings);
  const low = (
    <Shaper
      result={best}
      snippet={snippet}
      damping={damping}
      dampingHint={dampingHint}
      byChip={byChip(datasetVibrations)}
      heading={performance ? "For low vibrations" : undefined}
    />
  );
  const body = performance ? (
    <div className="grid gap-4 lg:grid-cols-2">
      {low}
      <Shaper
        result={performance}
        snippet={performanceSnippet}
        damping={damping}
        dampingHint={dampingHint}
        byChip={byChip(datasetVibrations.map((v, i) => ({ ...v, vibrs: performance.dataset_vibrs[i] ?? v.vibrs })))}
        heading="For performance"
      />
    </div>
  ) : (
    low
  );
  return <Panel title={title}>{body}</Panel>;
}
