import { useState } from "react";

import { useAxesMapAnalysis } from "@/analysis/useAxesMapAnalysis";
import { Waiting } from "@/components/Waiting";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { chipLabel } from "@/lib/format";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import type { Header } from "./SessionResults";
import { AxesMapChipTab, AxesMapSummary } from "./AxesMapChipTab";

export function AxesMapResults({ id, meta, header }: { id: string; meta: SessionMeta; header: Header }) {
  const query = useAxesMapAnalysis(id);
  const [chipName, setChipName] = useState<string | null>(null);

  if (query.isPending) {
    return (
      <div className="space-y-4">
        {header(null)}
        <Waiting>Analysing the calibration…</Waiting>
      </div>
    );
  }
  if (query.isError) {
    return (
      <div className="space-y-4">
        {header(null)}
        <p role="alert" className="rounded-lg border border-destructive p-4 text-sm text-destructive">
          The analysis failed: {query.error.message}
        </p>
      </div>
    );
  }
  const outcomes = query.data;
  const shown = outcomes.find((o) => o.chip === chipName) ?? outcomes[0];
  if (!shown) {
    return (
      <div className="space-y-4">
        {header(null)}
        <p className="text-sm">This session has no chips to analyse.</p>
      </div>
    );
  }
  const chip = meta.chips.find((c) => c.name === shown.chip) ?? null;
  const summary = (
    <div className="space-y-4">
      <Tabs value={shown.chip} onValueChange={setChipName}>
        <TabsList variant="line">
          {outcomes.map((o) => (
            <TabsTrigger key={o.chip} value={o.chip} className="after:bg-primary">
              {chipLabel(o.chip)}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
      {shown.state === "Analysed" && <AxesMapSummary outcome={shown} chip={chip} />}
    </div>
  );
  return (
    <div className="space-y-4">
      {header(null, summary)}
      <AxesMapChipTab outcome={shown} chip={chip} />
    </div>
  );
}
