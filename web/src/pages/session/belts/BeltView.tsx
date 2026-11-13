import type { useBelts } from "@/analysis/useBelts";
import { Panel } from "@/components/Panel";
import { Waiting } from "@/components/Waiting";
import { EChart } from "@/components/charts/EChart";
import { beltOverlayOption, beltScatterOption, beltSingleOption } from "@/components/charts/options";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { BeltSide } from "@/lib/generated/BeltSide";
import type { BeltsAnalysis } from "@/lib/generated/BeltsAnalysis";
import { chipLabel } from "@/lib/format";

import { BeltOffsetTable } from "./BeltOffsetTable";

/** As Shake&Tune names them. */
const BELT_LABEL = { a: "A (axis 1,-1)", b: "B (axis 1, 1)" } as const;

/** Why a recording that is there gave no spectrum, or nothing when it did. */
function reasonsOf(side: BeltSide): string[] {
  if (side.decision?.decision === "Excluded") return side.decision.reasons;
  const dropped = side.decision?.dropped;
  return dropped ? [dropped] : side.failure ? [side.failure] : [];
}

function Comparison({ analysis }: { analysis: BeltsAnalysis }) {
  const { result, chip } = analysis;
  if (!result) return null;
  return (
    <>
      <EChart option={beltOverlayOption(result)} label={`Belt overlay of ${chip}`} height={380} zoom />
      <BeltOffsetTable offsets={result.offsets} />
      <EChart option={beltScatterOption(result)} label={`Cross-belt scatter of ${chip}`} height={380} zoom />
    </>
  );
}

function SideStatus({
  belt,
  side,
  chip,
  onInclude,
}: {
  belt: "a" | "b";
  side: BeltSide;
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
      <EChart option={beltSingleOption(side.spectrum, label)} label={`Belt ${label} of ${chip}`} height={380} zoom />
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

function Outcome({ analysis, onInclude }: { analysis: BeltsAnalysis; onInclude: () => void }) {
  if (analysis.result) return <Comparison analysis={analysis} />;
  return (
    <>
      <p className="text-sm">There is no comparison without both belts.</p>
      {(["a", "b"] as const).map((belt) => (
        <SideStatus key={belt} belt={belt} side={analysis[`belt_${belt}`]} chip={analysis.chip} onInclude={onInclude} />
      ))}
    </>
  );
}

export function BeltView({
  chips,
  chip,
  onChipChange,
  query,
  onInclude,
}: {
  chips: string[];
  chip: string;
  onChipChange: (chip: string) => void;
  query: ReturnType<typeof useBelts>;
  onInclude: () => void;
}) {
  return (
    <Panel title="Belt comparison" label={`Chip ${chip}`}>
      <Tabs value={chip} onValueChange={onChipChange}>
        <TabsList variant="line">
          {chips.map((name) => (
            <TabsTrigger key={name} value={name} className="after:bg-primary">
              {chipLabel(name)}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
      {query.isPending ? (
        <Waiting>Comparing the belts of {chip}…</Waiting>
      ) : query.isError ? (
        <p role="alert" className="rounded-lg border border-destructive p-4 text-sm text-destructive">
          The belt comparison failed: {query.error.message}
        </p>
      ) : (
        <Outcome analysis={query.data} onInclude={onInclude} />
      )}
    </Panel>
  );
}
