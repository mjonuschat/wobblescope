import { useState } from "react";

import { useBelts } from "@/analysis/useBelts";
import { HintLabel } from "@/components/HintLabel";
import { Panel } from "@/components/Panel";
import { UnitInput } from "@/components/UnitInput";
import { Waiting } from "@/components/Waiting";
import { EChart } from "@/components/charts/EChart";
import { beltOverlayOption, beltScatterOption, beltSingleOption } from "@/components/charts/options";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { BeltSide } from "@/lib/generated/BeltSide";
import type { BeltsAnalysis } from "@/lib/generated/BeltsAnalysis";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import { parseNumber } from "@/lib/parse";

import { ScaleDialog } from "../ScaleDialog";
import { BeltOffsetTable } from "./BeltOffsetTable";

const DEFAULT_MAX_FREQ = 200;
const MIN_MAX_FREQ = 10;

/** As Shake&Tune names them. */
const BELT_LABEL = { a: "A (axis 1,-1)", b: "B (axis 1, 1)" } as const;

/** The analysis maximum from typed text; `undefined` when it is not a finite number of at least 10. */
function parseMaxFreq(text: string): number | undefined {
  const value = parseNumber(text);
  return value !== undefined && Number.isFinite(value) && value >= MIN_MAX_FREQ ? value : undefined;
}

/** Why a recording that is there gave no spectrum, or nothing when it did. */
function reasonsOf(side: BeltSide): string[] {
  if (side.decision?.decision === "Excluded") return side.decision.reasons;
  const dropped = side.decision?.dropped;
  return dropped ? [dropped] : side.failure ? [side.failure] : [];
}

function Similarity({ value, health }: { value: number; health: string }) {
  return (
    <p className="text-sm">
      Estimated similarity: {Number.isFinite(value) ? `${value.toFixed(1)}%` : "\u2013"} | {health} (experimental)
    </p>
  );
}

function Comparison({ analysis, maxScale }: { analysis: BeltsAnalysis; maxScale: number | null }) {
  const { result, chip } = analysis;
  if (!result) return null;
  return (
    <>
      <Similarity value={result.similarity} health={result.health} />
      <EChart option={beltOverlayOption(result, maxScale)} label={`Belt overlay of ${chip}`} height={380} />
      <BeltOffsetTable offsets={result.offsets} />
      <EChart option={beltScatterOption(result)} label={`Cross-belt scatter of ${chip}`} height={380} />
    </>
  );
}

function SideStatus({
  belt,
  side,
  maxScale,
  chip,
  onInclude,
}: {
  belt: "a" | "b";
  side: BeltSide;
  maxScale: number | null;
  chip: string;
  onInclude: () => void;
}) {
  const label = BELT_LABEL[belt];
  const name = belt.toUpperCase();
  if (!side.present) {
    return <p role="status" className="text-sm">{`Belt ${name} was not recorded.`}</p>;
  }
  if (side.spectrum) {
    return (
      <EChart
        option={beltSingleOption(side.spectrum, label, maxScale)}
        label={`Belt ${label} of ${chip}`}
        height={380}
      />
    );
  }
  const reasons = reasonsOf(side);
  return (
    <div role="status" className="space-y-2 text-sm">
      <p>{`Belt ${name} could not be used${reasons.length > 0 ? `: ${[...new Set(reasons)].join("; ")}` : ""}.`}</p>
      {side.decision?.decision === "Excluded" && (
        <Button type="button" variant="outline" size="sm" onClick={onInclude}>
          {`Include belt ${name} anyway`}
        </Button>
      )}
    </div>
  );
}

function Outcome({
  analysis,
  maxScale,
  onInclude,
}: {
  analysis: BeltsAnalysis;
  maxScale: number | null;
  onInclude: () => void;
}) {
  if (analysis.result) return <Comparison analysis={analysis} maxScale={maxScale} />;
  return (
    <>
      <p className="text-sm">There is no comparison without both belts.</p>
      {(["a", "b"] as const).map((belt) => (
        <SideStatus
          key={belt}
          belt={belt}
          side={analysis[`belt_${belt}`]}
          maxScale={maxScale}
          chip={analysis.chip}
          onInclude={onInclude}
        />
      ))}
    </>
  );
}

export function BeltView({ id, meta }: { id: string; meta: SessionMeta }) {
  const chips = meta.chips.map((c) => c.name);
  const [selected, setSelected] = useState<string | null>(null);
  const chip = selected && chips.includes(selected) ? selected : chips[0];
  const [maxFreqText, setMaxFreqText] = useState(String(DEFAULT_MAX_FREQ));
  const [maxFreq, setMaxFreq] = useState(DEFAULT_MAX_FREQ);
  const [includedChip, setIncludedChip] = useState<string | null>(null);
  // Where the overlay's y axis ends; `null` fits it to the data.
  const [maxScale, setMaxScale] = useState<number | null>(null);
  const query = useBelts({
    sessionId: id,
    meta,
    chip: chip ?? "",
    maxFreq,
    includeExcluded: includedChip === chip,
    enabled: chip !== undefined,
  });
  const parsed = parseMaxFreq(maxFreqText);

  if (chip === undefined) {
    return <p className="text-sm">This session has no recordings to analyse.</p>;
  }
  return (
    <Panel
      title="Belt comparison"
      label={`Chip ${chip}`}
      action={<ScaleDialog value={maxScale} onChange={setMaxScale} />}
    >
      <Tabs value={chip} onValueChange={setSelected}>
        <TabsList variant="line">
          {chips.map((name) => (
            <TabsTrigger key={name} value={name} className="after:bg-primary">
              {name}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
      <div className="flex max-w-xs flex-col gap-2.5">
        <HintLabel label="Analysis maximum (Hz)" hint="The comparison keeps the frequencies up to this." />
        <UnitInput
          unit="Hz"
          aria-label="Analysis maximum (Hz)"
          value={maxFreqText}
          aria-invalid={parsed === undefined}
          onChange={(event) => {
            setMaxFreqText(event.target.value);
            const next = parseMaxFreq(event.target.value);
            if (next !== undefined) setMaxFreq(next);
          }}
        />
        {parsed === undefined && (
          <p role="alert" className="text-sm text-destructive">
            Enter a number of at least {MIN_MAX_FREQ}.
          </p>
        )}
      </div>
      {/* The last valid result would otherwise stay up under the error. */}
      {parsed === undefined ? null : query.isPending ? (
        <Waiting>Comparing the belts of {chip}…</Waiting>
      ) : query.isError ? (
        <p role="alert" className="rounded-lg border border-destructive p-4 text-sm text-destructive">
          The belt comparison failed: {query.error.message}
        </p>
      ) : (
        <Outcome analysis={query.data} maxScale={maxScale} onInclude={() => setIncludedChip(chip)} />
      )}
    </Panel>
  );
}
