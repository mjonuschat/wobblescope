import { Maximize2Icon } from "lucide-react";
import type { ReactNode } from "react";

import { Panel } from "@/components/Panel";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
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

/** One chip's chart and shaper table, or why it has no recommendation. */
function ChipDetails({
  chip,
  snippets,
  maxFreq,
  decisions,
  chartHeight,
}: Omit<Props, "switcher"> & { chartHeight?: number | string }) {
  const rec = chip.recommendation;
  const reasons = decisions
    .filter((d) => d.chip === chip.chip)
    .flatMap((d) => (d.decision === "Excluded" ? d.reasons : d.dropped !== null ? [d.dropped] : []));
  return (
    <>
      <EChart
        option={psdOption({ chip, recommendation: rec, maxFreq })}
        label={`PSD of ${chip.chip}${rec ? `, recommended ${rec.best.name} at ${formatHz(rec.best.freq)} Hz` : ""}`}
        height={chartHeight}
      />
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
  return (
    <Panel
      title="Resonance and Shapers"
      label={`Chip ${chip.chip}`}
      action={
        <Dialog>
          <DialogTrigger
            render={<Button type="button" variant="outline" size="icon" aria-label="Expand the chart and table" />}
          >
            <Maximize2Icon />
          </DialogTrigger>
          <DialogContent className="h-[calc(100vh-4rem)] gap-4 overflow-auto sm:max-w-[calc(100vw-4rem)]">
            <DialogHeader>
              <DialogTitle>Resonance and Shapers: {chip.chip}</DialogTitle>
            </DialogHeader>
            <div className="space-y-4">
              <ChipDetails {...details} chartHeight="min(60vh, 640px)" />
            </div>
          </DialogContent>
        </Dialog>
      }
    >
      {switcher}
      <ChipDetails {...details} />
    </Panel>
  );
}
