import type { ReactNode } from "react";

import { EXPANDED_CHART_HEIGHT, ExpandDialog } from "@/components/ExpandDialog";
import { Panel } from "@/components/Panel";
import { EChart } from "@/components/charts/EChart";
import { psdOption } from "@/components/charts/options";
import { formatHz } from "@/lib/format";
import type { ChipAnalysis } from "@/lib/generated/ChipAnalysis";
import type { DatasetDecision } from "@/lib/generated/DatasetDecision";

import { ShaperTable } from "./ShaperTable";

interface Props {
  chip: ChipAnalysis;
  switcher: ReactNode;
  snippets: string[] | null;
  maxFreq: number;
  decisions: DatasetDecision[];
}

function ChipDetails({
  chip,
  snippets,
  maxFreq,
  decisions,
  chartHeight,
  corner,
}: Omit<Props, "switcher"> & {
  chartHeight?: number | string;
  corner?: ReactNode;
}) {
  const rec = chip.recommendation;
  const reasons = decisions
    .filter((d) => d.chip === chip.chip)
    .flatMap((d) => (d.decision === "Excluded" ? d.reasons : d.dropped !== null ? [d.dropped] : []));
  return (
    <>
      <div className="relative">
        <EChart
          option={psdOption({ chip, recommendation: rec, maxFreq })}
          label={`PSD of ${chip.chip}${rec ? `, recommended ${rec.best.name} at ${formatHz(rec.best.freq)} Hz` : ""}`}
          height={chartHeight}
        />
        {corner && <div className="absolute top-0 right-0">{corner}</div>}
      </div>
      {rec ? (
        <ShaperTable recommendation={rec} snippets={snippets} />
      ) : (
        <p className="text-sm">
          No recommendation for this chip{reasons.length > 0 && `: ${[...new Set(reasons)].join("; ")}`}.
        </p>
      )}
    </>
  );
}

export function ChipResult({ chip, switcher, snippets, maxFreq, decisions }: Props) {
  const details = { chip, snippets, maxFreq, decisions };
  const expand = (
    <ExpandDialog title={`Resonance and Shapers: ${chip.chip}`}>
      <ChipDetails {...details} chartHeight={EXPANDED_CHART_HEIGHT} />
    </ExpandDialog>
  );
  return (
    <Panel title="Resonance and Shapers" label={`Chip ${chip.chip}`}>
      {switcher}
      <ChipDetails {...details} corner={chip.recommendation ? expand : undefined} />
    </Panel>
  );
}
