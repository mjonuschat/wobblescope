import { type ReactNode, useState } from "react";

import { EXPANDED_CHART_HEIGHT, ExpandDialog } from "@/components/ExpandDialog";
import { Panel } from "@/components/Panel";
import { EChart } from "@/components/charts/EChart";
import { psdOption } from "@/components/charts/options";
import { formatHz } from "@/lib/format";
import type { ChipAnalysis } from "@/lib/generated/ChipAnalysis";
import type { DatasetDecision } from "@/lib/generated/DatasetDecision";

import { ScaleDialog } from "./ScaleDialog";
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
  maxScale,
  chartHeight,
  corner,
}: Omit<Props, "switcher"> & {
  /** The left axis of the PSD chart ends here; `null` scales it to the data. */
  maxScale: number | null;
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
          option={psdOption({ chip, recommendation: rec, maxFreq, maxScale })}
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
  // Where the vibration axis ends. `null` fits it to the data.
  const [maxScale, setMaxScale] = useState<number | null>(null);
  const details = { chip, snippets, maxFreq, decisions, maxScale };
  const expand = (
    <ExpandDialog title={`Resonance and Shapers: ${chip.chip}`}>
      <ChipDetails {...details} chartHeight={EXPANDED_CHART_HEIGHT} />
    </ExpandDialog>
  );
  return (
    <Panel title="Resonance and Shapers" label={`Chip ${chip.chip}`}>
      {switcher}
      <ChipDetails
        {...details}
        corner={
          <div className="flex gap-2">
            <ScaleDialog value={maxScale} onChange={setMaxScale} />
            {chip.recommendation && expand}
          </div>
        }
      />
    </Panel>
  );
}
