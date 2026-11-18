import type { useBelts } from "@/analysis/useBelts";
import { Panel } from "@/ui-kit/Panel";
import { Waiting } from "@/components/Waiting";
import { EChart } from "@/components/charts/EChart";
import { beltOverlayOption, beltScatterOption, beltSingleOption } from "@/components/charts/options";
import { Button } from "@/components/ui/button";
import type { BeltSide } from "@/lib/generated/BeltSide";
import type { BeltsAnalysis } from "@/lib/generated/BeltsAnalysis";
import { chipLabel } from "@/lib/format";

import { BeltOffsetTable } from "./BeltOffsetTable";
import { useChartTheme } from "@/ui-kit/chartTheme";
import { Alert } from "@/ui-kit/Alert";

/** As Shake&Tune names them. */
const BELT_LABEL = { a: "A (axis 1,-1)", b: "B (axis 1, 1)" } as const;

/** Why a recording that is there gave no spectrum, or nothing when it did. */
function reasonsOf(side: BeltSide): string[] {
  if (side.decision?.decision === "Excluded") return side.decision.reasons;
  const dropped = side.decision?.dropped;
  return dropped ? [dropped] : side.failure ? [side.failure] : [];
}

function Profile({ analysis }: { analysis: BeltsAnalysis }) {
  const theme = useChartTheme();
  const { result, chip } = analysis;
  if (!result) return null;
  return (
    <>
      <EChart option={beltOverlayOption(result, theme)} label={`Belt overlay of ${chip}`} height={380} zoom />
      {result.offsets.length > 0 && <BeltOffsetTable offsets={result.offsets} />}
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
  const theme = useChartTheme();
  const label = BELT_LABEL[belt];
  const name = belt.toUpperCase();
  if (!side.present) {
    return <p role="status" className="text-sm">{`Belt ${name} was not recorded.`}</p>;
  }
  if (side.spectrum) {
    return (
      <EChart
        option={beltSingleOption(side.spectrum, label, theme)}
        label={`Belt ${label} of ${chip}`}
        height={380}
        zoom
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

function Outcome({ analysis, onInclude }: { analysis: BeltsAnalysis; onInclude: () => void }) {
  if (analysis.result) return <Profile analysis={analysis} />;
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
  chip,
  query,
  onInclude,
}: {
  chip: string;
  query: ReturnType<typeof useBelts>;
  onInclude: () => void;
}) {
  const theme = useChartTheme();
  const result = query.data?.result;
  return (
    <>
      <Panel title="Belt frequency profile" label={`Belt frequency profile of ${chipLabel(chip)}`}>
        {query.isPending ? (
          <Waiting>Comparing the belts of {chip}…</Waiting>
        ) : query.isError ? (
          <Alert tone="danger">The belt comparison failed: {query.error.message}</Alert>
        ) : (
          <Outcome analysis={query.data} onInclude={onInclude} />
        )}
      </Panel>
      {result && (
        <Panel title="Cross-belt comparison" label={`Cross-belt comparison of ${chipLabel(chip)}`}>
          {result.offsets.length === 0 && <p className="text-sm">No peaks were paired across the belts.</p>}
          <EChart option={beltScatterOption(result, theme)} label={`Cross-belt scatter of ${chip}`} height={380} zoom />
        </Panel>
      )}
    </>
  );
}
