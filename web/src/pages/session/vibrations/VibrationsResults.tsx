import { type ReactNode, useState } from "react";

import { useVibrations } from "@/analysis/useVibrations";
import { Panel } from "@/components/Panel";
import { Waiting } from "@/components/Waiting";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { chipLabel } from "@/lib/format";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import type { Header } from "../SessionResults";
import { VibrationsSummary } from "./VibrationsSummary";
import { VibrationsView } from "./VibrationsView";
import { vibrationsProgress } from "./vibrationsProgress";

/** The value of the tab that combines the chips. No chip is named like this. */
const ALL_CHIPS = "\u0000all-chips";

function IncompleteRun({ meta }: { meta: SessionMeta }) {
  const reason =
    meta.outcome === "Complete"
      ? null
      : "Incomplete" in meta.outcome
        ? meta.outcome.Incomplete.reason
        : meta.outcome.Failed.reason;
  const progress = vibrationsProgress(meta);
  return (
    <Panel title="Vibrations Profile">
      <p role="status" className="text-sm">
        The run did not finish, so there is no profile to show.
      </p>
      {progress && <p className="text-sm">{progress}</p>}
      {reason && <p className="text-sm text-muted-foreground">{`Reason: ${reason}`}</p>}
    </Panel>
  );
}

export function VibrationsResults({ id, meta, header }: { id: string; meta: SessionMeta; header: Header }) {
  const complete = meta.outcome === "Complete";
  const [selected, setSelected] = useState(ALL_CHIPS);
  const [includeExcluded, setIncludeExcluded] = useState(false);
  const query = useVibrations({ sessionId: id, meta, includeExcluded, enabled: complete });

  if (!complete) {
    return (
      <div className="space-y-4">
        {header(null)}
        <IncompleteRun meta={meta} />
      </div>
    );
  }

  const data = query.data;
  const chips = data ? data.per_chip.map((c) => c.chip) : [];
  const tab = chips.includes(selected) ? selected : ALL_CHIPS;
  const shown = data
    ? tab === ALL_CHIPS
      ? data.worst_case
      : (data.per_chip.find((c) => c.chip === tab)?.result ?? null)
    : null;
  const summary: ReactNode = data ? (
    <div className="space-y-4">
      <Tabs value={tab} onValueChange={setSelected}>
        <TabsList variant="line">
          <TabsTrigger value={ALL_CHIPS} className="after:bg-primary">
            All Chips
          </TabsTrigger>
          {chips.map((name) => (
            <TabsTrigger key={name} value={name} className="after:bg-primary">
              {chipLabel(name)}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
      {shown && <VibrationsSummary result={shown} />}
    </div>
  ) : null;

  return (
    <div className="space-y-4">
      {header(null, summary)}
      {query.isPending ? (
        <Waiting>Profiling the vibrations…</Waiting>
      ) : query.isError ? (
        <p role="alert" className="rounded-lg border border-destructive p-4 text-sm text-destructive">
          The vibrations analysis failed: {query.error.message}
        </p>
      ) : (
        <VibrationsView
          perChip={query.data.per_chip}
          combined={query.data.worst_case}
          chip={tab === ALL_CHIPS ? null : tab}
          kinematics={meta.vibrations_run?.kinematics ?? ""}
          onInclude={() => setIncludeExcluded(true)}
        />
      )}
    </div>
  );
}
