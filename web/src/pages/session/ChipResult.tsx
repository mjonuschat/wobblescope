import { type ReactNode, useMemo, useState } from "react";

import { EXPANDED_CHART_HEIGHT, ExpandDialog } from "@/components/ExpandDialog";
import { Panel } from "@/ui-kit/Panel";
import { EChart } from "@/components/charts/EChart";
import { WIDE_CHART } from "@/components/charts/size";
import { psdOption } from "@/components/charts/options";
import { formatHz } from "@/lib/format";
import type { ChipAnalysis } from "@/lib/generated/ChipAnalysis";
import type { DatasetDecision } from "@/lib/generated/DatasetDecision";

import { ScaleDialog } from "./ScaleDialog";
import { ShaperTable } from "./ShaperTable";
import { useChartTheme } from "@/ui-kit/chartTheme";

export interface SharedScale {
  value: number | null;
  onChange: (value: number | null) => void;
  auto?: number;
}

interface Props {
  chip: ChipAnalysis;
  switcher: ReactNode;
  snippets: string[] | null;
  maxFreq: number;
  decisions: DatasetDecision[];
  /** Replaces the chart's own scale, so two charts can be compared at one scale. */
  scale?: SharedScale;
}

function ChipDetails({
  chip,
  snippets,
  maxFreq,
  decisions,
  maxScale,
  autoScale,
  chartHeight,
  corner,
}: Omit<Props, "switcher" | "scale"> & {
  /** The left axis of the PSD chart ends here; `null` scales it to the data. */
  maxScale: number | null;
  /** The scale to use when `maxScale` is `null`, in place of one fitted to this chart's data. */
  autoScale?: number;
  chartHeight?: number | string;
  corner?: ReactNode;
}) {
  const theme = useChartTheme();
  const rec = chip.recommendation;
  // The same option on a re-render keeps the legend choices.
  const option = useMemo(
    () => psdOption({ chip, recommendation: rec, maxFreq, maxScale: maxScale ?? autoScale ?? null, theme }),
    [chip, rec, maxFreq, maxScale, autoScale, theme],
  );
  const reasons = decisions
    .filter((d) => d.chip === chip.chip)
    .flatMap((d) => (d.decision === "Excluded" ? d.reasons : d.dropped !== null ? [d.dropped] : []));
  return (
    <>
      <div className="relative">
        <EChart
          option={option}
          label={`PSD of ${chip.chip}${rec ? `, recommended ${rec.best.name} at ${formatHz(rec.best.freq)} Hz` : ""}`}
          height={chartHeight}
          minWidth={WIDE_CHART}
          legendSwitches={["Axes", "Shapers"]}
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

export function ChipResult({ chip, switcher, snippets, maxFreq, decisions, scale }: Props) {
  // Where the vibration axis ends. `null` fits it to the data.
  const [ownScale, setOwnScale] = useState<number | null>(null);
  const maxScale = scale ? scale.value : ownScale;
  const setMaxScale = scale ? scale.onChange : setOwnScale;
  const details = { chip, snippets, maxFreq, decisions, maxScale, autoScale: scale?.auto };
  const expand = (
    <ExpandDialog title={`Resonance and shapers: ${chip.chip}`}>
      <ChipDetails {...details} chartHeight={EXPANDED_CHART_HEIGHT} />
    </ExpandDialog>
  );
  return (
    <Panel title="Resonance and shapers" label={`Chip ${chip.chip}`}>
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
