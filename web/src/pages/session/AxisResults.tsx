import { EChart } from "@/components/charts/EChart";
import { summedOption } from "@/components/charts/options";
import type { Analysis } from "@/lib/generated/Analysis";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import type { AnalysisResult } from "@/analysis/protocol";

import { ChipResult } from "./ChipResult";
import { EligibilityPanel } from "./EligibilityPanel";
import { RecommendationCard } from "./RecommendationCard";

function effectiveSettings(analysis: Analysis) {
  return (
    analysis.worst_case?.settings ?? analysis.per_chip.find((c) => c.recommendation)?.recommendation?.settings ?? null
  );
}

interface Props {
  result: AnalysisResult;
  meta: SessionMeta;
  overrides: string[];
  onOverridesChange: (overrides: string[]) => void;
}

export function AxisResults({ result, meta, overrides, onOverridesChange }: Props) {
  const { analysis, snippets } = result;
  const settings = effectiveSettings(analysis);
  const maxFreq = settings?.max_freq ?? 200;
  return (
    <div className="space-y-4">
      {analysis.worst_case ? (
        <RecommendationCard
          title="Worst case across all chips"
          recommendation={analysis.worst_case}
          snippet={snippets.worstCase}
          datasetVibrations={analysis.worst_case_vibrations}
          probePoints={meta.probe_points}
        />
      ) : (
        <p role="status" className="rounded-lg border p-4 text-sm">
          No recommendation for this axis: none of its recordings could be used. The reasons are listed below.
        </p>
      )}
      <EligibilityPanel
        analysis={analysis}
        probePoints={meta.probe_points}
        overrides={overrides}
        onOverridesChange={onOverridesChange}
      />
      {analysis.per_chip.map((chip, i) => (
        <ChipResult
          key={chip.chip}
          chip={chip}
          snippet={snippets.perChip[i]}
          probePoints={meta.probe_points}
          maxFreq={maxFreq}
          decisions={analysis.eligibility.decisions}
        />
      ))}
      {analysis.summed && (
        <section className="space-y-2 rounded-lg border p-4" aria-label="Summed PSD">
          <EChart
            option={summedOption(analysis.summed, maxFreq)}
            label="Summed PSD of the eligible chips, for display only"
          />
          <p className="text-xs text-muted-foreground">
            The sum is for looking at. No recommendation comes from it. Like every spectrum on this page it is
            frequency-normalised (divided by frequency plus 0.1 Hz, damped below 10 Hz), as in Klipper's graphs, so its
            values are not raw PSD and chips of different sensitivity add up unevenly.
          </p>
        </section>
      )}
      <p className="text-xs text-muted-foreground">
        Analysed with core {analysis.core_version}
        {settings &&
          `. Fit: corner velocity ${settings.scv} mm/s, damping ratio ${settings.damping_ratio}, ` +
            `smoothing limit ${settings.max_smoothing ?? "none"}, vibration limit ${settings.max_vibrations ?? "default"}, ` +
            `shapers ${settings.shapers.join(", ")}, up to ${settings.max_freq} Hz.`}
      </p>
    </div>
  );
}
