import { useState } from "react";

import { useSessionAnalysis } from "@/analysis/useSessionAnalysis";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useSession } from "@/lib/api/queries";
import type { Axis } from "@/lib/generated/Axis";
import type { FitParams } from "@/lib/generated/FitParams";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import { AxisResults } from "./AxisResults";
import { FitSettingsForm } from "./FitSettingsForm";
import { MetaPanel } from "./MetaPanel";
import { defaultFitParams } from "./fit";

interface AnalysisProps {
  id: string;
  meta: SessionMeta;
  axis: Axis;
  params: FitParams;
  overrides: string[];
  onOverridesChange: (overrides: string[]) => void;
}

function Analysis({ id, meta, axis, params, overrides, onOverridesChange }: AnalysisProps) {
  const analysis = useSessionAnalysis({ sessionId: id, meta, axis, params, overrides });
  if (analysis.isPending) {
    return <p className="text-sm text-muted-foreground">Analysing axis {axis}…</p>;
  }
  if (analysis.isError) {
    return (
      <p role="alert" className="rounded-lg border border-destructive p-4 text-sm text-destructive">
        The analysis failed: {analysis.error.message}
      </p>
    );
  }
  return <AxisResults result={analysis.data} meta={meta} overrides={overrides} onOverridesChange={onOverridesChange} />;
}

/**
 * A stored session's analysis: a tab for each axis, the recommendation and charts for the one
 * chosen, the fit settings and the details of the recording. The session page and the results
 * of a test show the same thing.
 */
export function SessionResults({ id }: { id: string }) {
  const session = useSession(id);
  const [selected, setSelected] = useState<Axis | null>(null);
  const [params, setParams] = useState<FitParams>(defaultFitParams);
  const [overrides, setOverrides] = useState<string[]>([]);

  if (session.isPending) return <p className="text-sm text-muted-foreground">Loading session…</p>;
  if (session.isError) {
    return (
      <p role="alert" className="text-sm text-destructive">
        {session.error.message}
      </p>
    );
  }

  const { meta } = session.data;
  const axes = [...new Set(meta.segments.map((s) => s.axis))];
  const axis = selected && axes.includes(selected) ? selected : (axes[0] ?? null);

  return (
    <div className="space-y-4">
      {axis === null ? (
        <p className="text-sm">This session has no recordings to analyse.</p>
      ) : (
        <>
          <Tabs value={axis} onValueChange={(value) => setSelected(value as Axis)}>
            <TabsList>
              {axes.map((a) => (
                <TabsTrigger key={a} value={a}>
                  Axis {a}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
          <Analysis
            id={id}
            meta={meta}
            axis={axis}
            params={params}
            overrides={overrides}
            onOverridesChange={setOverrides}
          />
          <FitSettingsForm onApply={setParams} />
        </>
      )}
      <MetaPanel meta={meta} />
    </div>
  );
}
