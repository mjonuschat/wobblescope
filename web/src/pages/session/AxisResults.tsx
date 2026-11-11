import { useState } from "react";

import { Panel } from "@/components/Panel";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { Analysis } from "@/lib/generated/Analysis";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import type { AnalysisResult } from "@/analysis/protocol";

import { ChipResult } from "./ChipResult";
import { CombinedResult } from "./CombinedResult";
import { EligibilityPanel } from "./EligibilityPanel";
import { RecommendationCard } from "./RecommendationCard";

/** The value of the tab that shows all the chips added together. */
const ALL_CHIPS = "*all-chips*";

/** True in the dev server when the URL asks for fake data, so the combined view can be looked at. */
const previewing = () => import.meta.env.DEV && /[?&]preview=/.test(window.location.hash);

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
  // The tab that adds the chips together is for several chips; the dev preview shows it for one.
  const showAllTab = analysis.summed !== null && (analysis.per_chip.length >= 2 || previewing());
  const showAll = showAllTab && chipName === ALL_CHIPS;
  const switcher = shown && (
    <Tabs value={showAll ? ALL_CHIPS : shown.chip} onValueChange={setChipName}>
      <TabsList variant="line">
        {analysis.per_chip.map((c) => (
          <TabsTrigger key={c.chip} value={c.chip} className="after:bg-primary">
            {c.chip}
          </TabsTrigger>
        ))}
        {showAllTab && (
          <TabsTrigger value={ALL_CHIPS} className="after:bg-primary">
            All Chips
          </TabsTrigger>
        )}
      </TabsList>
    </Tabs>
  );
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
      {showAll && analysis.summed ? (
        <CombinedResult summed={analysis.summed} maxFreq={maxFreq} switcher={switcher} />
      ) : (
        shown && (
          <ChipResult
            chip={shown}
            switcher={switcher}
            snippets={snippets.perChip[shownIndex]}
            maxFreq={maxFreq}
            decisions={analysis.eligibility.decisions}
          />
        )
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
