import { useState } from "react";

import { Panel } from "@/components/Panel";
import { EChart } from "@/components/charts/EChart";
import { summedOption } from "@/components/charts/options";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
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
  // The chip whose chart and table show. A name that is not in this axis's analysis gives way to the first.
  const [chipName, setChipName] = useState<string | null>(null);
  const shownIndex = Math.max(
    0,
    analysis.per_chip.findIndex((c) => c.chip === chipName),
  );
  const shown = analysis.per_chip[shownIndex];
  return (
    <div className="space-y-4">
      {analysis.worst_case ? (
        <RecommendationCard
          title="Recommended Input Shaper"
          recommendation={analysis.worst_case}
          snippet={snippets.worstCase}
          datasetVibrations={analysis.worst_case_vibrations}
          probePoints={meta.probe_points}
        />
      ) : (
        <Panel title="Recommendation">
          <p role="status" className="text-sm">
            No recommendation for this axis: none of its recordings could be used. The reasons are listed below.
          </p>
        </Panel>
      )}
      <EligibilityPanel
        analysis={analysis}
        probePoints={meta.probe_points}
        overrides={overrides}
        onOverridesChange={onOverridesChange}
      />
      {analysis.per_chip.length > 1 && (
        <Tabs value={shown.chip} onValueChange={setChipName}>
          <TabsList variant="line">
            {analysis.per_chip.map((c) => (
              <TabsTrigger key={c.chip} value={c.chip} className="after:bg-primary">
                {c.chip}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
      )}
      {shown && (
        <ChipResult
          key={shown.chip}
          chip={shown}
          snippets={snippets.perChip[shownIndex]}
          probePoints={meta.probe_points}
          maxFreq={maxFreq}
          decisions={analysis.eligibility.decisions}
        />
      )}
      {analysis.summed && (
        <Panel title="Summed PSD" label="Summed PSD">
          <EChart
            option={summedOption(analysis.summed, maxFreq)}
            label="Summed PSD of the eligible chips, for display only"
          />
          <p className="text-xs text-muted-foreground">
            The sum is for looking at. No recommendation comes from it. Like every spectrum on this page it is
            frequency-normalised (divided by frequency plus 0.1 Hz, damped below 10 Hz), as in Klipper's graphs, so its
            values are not raw PSD and chips of different sensitivity add up unevenly.
          </p>
        </Panel>
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
