import { type ReactNode, useState } from "react";

import { useBelts } from "@/analysis/useBelts";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { chipLabel } from "@/lib/format";
import type { BeltImbalance } from "@/lib/generated/BeltImbalance";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import type { Header } from "../SessionResults";
import { BeltView } from "./BeltView";
import { healthGrade } from "./healthGrade";

/** Shake&Tune's default; the chart zoom covers closer looks. */
const MAX_FREQ = 200;

/** One sentence on how unequal the belts' power is; the score is only scaled when it was found. */
function imbalanceText({ state, figure, lo_hz, hi_hz, stronger }: BeltImbalance): string {
  if (state === "Unavailable") return "Level balance unavailable. The health score is not scaled.";
  if (state === "NoneFound" || stronger === null)
    return "No sustained imbalance found. The health score is not scaled.";
  const weaker = stronger === "A" ? "B" : "A";
  const range = `${Math.round(lo_hz)} to ${Math.round(hi_hz)} Hz`;
  if (figure <= 0) return `There is no power in belt ${weaker} in most of this range, ${range}`;
  return `Belt ${stronger} has a median of ${(1 / figure).toFixed(1)}x belt ${weaker}'s power from ${range}`;
}

/** The similarity, belt level and health rows, styled like the progress panel's labelled rows. */
function BeltSummary({
  similarity,
  health,
  imbalance,
}: {
  similarity: number;
  health: string;
  imbalance: BeltImbalance;
}) {
  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
      <dt className="text-muted-foreground">Estimated similarity</dt>
      <dd>{Number.isFinite(similarity) ? `${similarity.toFixed(1)}%` : "\u2013"}</dd>
      <dt className="text-muted-foreground">Belt level</dt>
      <dd>{imbalanceText(imbalance)}</dd>
      <dt className="text-muted-foreground">Mechanical health</dt>
      <dd>{healthGrade(health)} (experimental)</dd>
    </dl>
  );
}

/** The chip switcher; it sits in the Result card and drives both cards below it. */
function ChipTabs({ chips, chip, onChange }: { chips: string[]; chip: string; onChange: (chip: string) => void }) {
  return (
    <Tabs value={chip} onValueChange={onChange}>
      <TabsList variant="line">
        {chips.map((name) => (
          <TabsTrigger key={name} value={name} className="after:bg-primary">
            {chipLabel(name)}
          </TabsTrigger>
        ))}
      </TabsList>
    </Tabs>
  );
}

/**
 * Owns the chip so the top card and the comparison card below it show the same
 * analysis.
 */
export function BeltResults({ id, meta, header }: { id: string; meta: SessionMeta; header: Header }) {
  const chips = meta.chips.map((c) => c.name);
  const [selected, setSelected] = useState<string | null>(null);
  const chip = selected && chips.includes(selected) ? selected : chips[0];
  const [includedChip, setIncludedChip] = useState<string | null>(null);
  const query = useBelts({
    sessionId: id,
    meta,
    chip: chip ?? "",
    maxFreq: MAX_FREQ,
    includeExcluded: includedChip === chip,
    enabled: chip !== undefined,
  });

  const result = query.data ? query.data.result : null;
  const summary: ReactNode =
    chip === undefined ? null : (
      <div className="space-y-4">
        <ChipTabs chips={chips} chip={chip} onChange={setSelected} />
        {result && <BeltSummary similarity={result.similarity} health={result.health} imbalance={result.imbalance} />}
      </div>
    );

  return (
    <div className="space-y-4">
      {header(null, summary)}
      {chip === undefined ? (
        <p className="text-sm">This session has no recordings to analyse.</p>
      ) : (
        <BeltView chip={chip} query={query} onInclude={() => setIncludedChip(chip)} />
      )}
    </div>
  );
}
