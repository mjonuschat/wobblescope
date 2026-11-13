import { type ReactNode, useState } from "react";

import { useBelts } from "@/analysis/useBelts";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import type { Header } from "../SessionResults";
import { BeltView } from "./BeltView";
import { healthGrade } from "./healthGrade";

/** Shake&Tune's default; the chart zoom covers closer looks. */
const MAX_FREQ = 200;

/** The similarity and health rows, styled like the progress panel's labelled rows. */
function BeltSummary({ similarity, health }: { similarity: number; health: string }) {
  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
      <dt className="text-muted-foreground">Estimated similarity</dt>
      <dd>{Number.isFinite(similarity) ? `${similarity.toFixed(1)}%` : "\u2013"}</dd>
      <dt className="text-muted-foreground">Mechanical health</dt>
      <dd>{healthGrade(health)} (experimental)</dd>
    </dl>
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
  const summary: ReactNode = result ? <BeltSummary similarity={result.similarity} health={result.health} /> : null;

  return (
    <div className="space-y-4">
      {header(null, summary)}
      {chip === undefined ? (
        <p className="text-sm">This session has no recordings to analyse.</p>
      ) : (
        <BeltView
          chips={chips}
          chip={chip}
          onChipChange={setSelected}
          query={query}
          onInclude={() => setIncludedChip(chip)}
        />
      )}
    </div>
  );
}
