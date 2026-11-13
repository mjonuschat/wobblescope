import { useState } from "react";

import { useAxesMapAnalysis } from "@/analysis/useAxesMapAnalysis";
import { Waiting } from "@/components/Waiting";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import { AxesMapChipTab } from "./AxesMapChipTab";

/** The outcome of an axes-map calibration session, one tab per chip. */
export function AxesMapResults({ id, meta }: { id: string; meta: SessionMeta }) {
  const query = useAxesMapAnalysis(id);
  const [chipName, setChipName] = useState<string | null>(null);

  if (query.isPending) {
    return <Waiting>Analysing the calibration…</Waiting>;
  }
  if (query.isError) {
    return (
      <p role="alert" className="rounded-lg border border-destructive p-4 text-sm text-destructive">
        The analysis failed: {query.error.message}
      </p>
    );
  }
  const outcomes = query.data;
  const shown = outcomes.find((o) => o.chip === chipName) ?? outcomes[0];
  if (!shown) {
    return <p className="text-sm">This session has no chips to analyse.</p>;
  }
  return (
    <div className="space-y-4">
      <Tabs value={shown.chip} onValueChange={setChipName}>
        <TabsList variant="line">
          {outcomes.map((o) => (
            <TabsTrigger key={o.chip} value={o.chip} className="after:bg-primary">
              {o.chip}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
      <AxesMapChipTab outcome={shown} chip={meta.chips.find((c) => c.name === shown.chip) ?? null} />
    </div>
  );
}
