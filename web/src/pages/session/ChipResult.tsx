import { Panel } from "@/components/Panel";
import { EChart } from "@/components/charts/EChart";
import { psdOption } from "@/components/charts/options";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { formatHz } from "@/lib/format";
import type { ChipAnalysis } from "@/lib/generated/ChipAnalysis";
import type { DatasetDecision } from "@/lib/generated/DatasetDecision";
import type { ProbePoint } from "@/lib/generated/ProbePoint";

import { RecommendationCard } from "./RecommendationCard";
import { ShaperTable } from "./ShaperTable";

interface Props {
  chip: ChipAnalysis;
  snippet: string | null;
  probePoints: ProbePoint[];
  maxFreq: number;
  decisions: DatasetDecision[];
}

export function ChipResult({ chip, snippet, probePoints, maxFreq, decisions }: Props) {
  const rec = chip.recommendation;
  const reasons = decisions
    .filter((d) => d.chip === chip.chip)
    .flatMap((d) => (d.decision === "Excluded" ? d.reasons : d.dropped !== null ? [d.dropped] : []));
  const option = psdOption({ chip, recommendation: rec, probePoints, maxFreq });
  return (
    <Panel
      title={chip.chip}
      label={`Chip ${chip.chip}`}
      action={<Badge variant={chip.health === "Degraded" ? "destructive" : "secondary"}>{chip.health}</Badge>}
    >
      <EChart
        option={option}
        label={`PSD of ${chip.chip}${rec ? `, recommended ${rec.best.name} at ${formatHz(rec.best.freq)} Hz` : ""}`}
      />
      <p className="text-xs text-muted-foreground">
        Spectra are frequency-normalised as in Klipper's graphs, not raw PSD.
      </p>
      {rec ? (
        <>
          <ShaperTable recommendation={rec} />
          <Separator />
          <RecommendationCard title={`Recommendation for ${chip.chip}`} recommendation={rec} snippet={snippet} bare />
        </>
      ) : (
        <p className="text-sm">
          No recommendation for this chip{reasons.length > 0 && `: ${[...new Set(reasons)].join("; ")}`}.
        </p>
      )}
    </Panel>
  );
}
