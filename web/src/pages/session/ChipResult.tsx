import type { ReactNode } from "react";

import { Panel } from "@/components/Panel";
import { EChart } from "@/components/charts/EChart";
import { psdOption } from "@/components/charts/options";
import { formatHz } from "@/lib/format";
import type { ChipAnalysis } from "@/lib/generated/ChipAnalysis";
import type { DatasetDecision } from "@/lib/generated/DatasetDecision";

import { ShaperTable } from "./ShaperTable";

interface Props {
  chip: ChipAnalysis;
  /** The control that switches between chips, shown at the top of the card. */
  switcher: ReactNode;
  /** The `[input_shaper]` snippet of each shaper in the recommendation, in order. */
  snippets: string[] | null;
  maxFreq: number;
  decisions: DatasetDecision[];
}

export function ChipResult({ chip, switcher, snippets, maxFreq, decisions }: Props) {
  const rec = chip.recommendation;
  const reasons = decisions
    .filter((d) => d.chip === chip.chip)
    .flatMap((d) => (d.decision === "Excluded" ? d.reasons : d.dropped !== null ? [d.dropped] : []));
  const option = psdOption({ chip, recommendation: rec, maxFreq });
  return (
    <Panel title="Resonance and Shapers" label={`Chip ${chip.chip}`}>
      {switcher}
      <EChart
        option={option}
        label={`PSD of ${chip.chip}${rec ? `, recommended ${rec.best.name} at ${formatHz(rec.best.freq)} Hz` : ""}`}
      />
      {rec ? (
        <ShaperTable recommendation={rec} snippets={snippets} />
      ) : (
        <p className="text-sm">
          No recommendation for this chip{reasons.length > 0 && `: ${[...new Set(reasons)].join("; ")}`}.
        </p>
      )}
    </Panel>
  );
}
