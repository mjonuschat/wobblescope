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
import { SaveResultDialog } from "./SaveResultDialog";
import { baselineFor } from "./fit";

type AnalysisQuery = ReturnType<typeof useSessionAnalysis>;

/** The analysis of the chosen axis: a wait, the reason it failed, or the results. */
function AnalysisView({
  query,
  id,
  axis,
  meta,
  params,
  overrides,
  onOverridesChange,
}: {
  query: AnalysisQuery;
  id: string;
  axis: Axis;
  params: FitParams;
  meta: SessionMeta;
  overrides: string[];
  onOverridesChange: (overrides: string[]) => void;
}) {
  if (query.isPending) {
    return <p className="text-sm text-muted-foreground">Analysing axis {axis}…</p>;
  }
  if (query.isError) {
    return (
      <p role="alert" className="rounded-lg border border-destructive p-4 text-sm text-destructive">
        The analysis failed: {query.error.message}
      </p>
    );
  }
  return (
    <div className="space-y-4">
      <SaveResultDialog
        sessionId={id}
        meta={meta}
        axis={axis}
        params={params}
        overrides={overrides}
        result={query.data}
      />
      <AxisResults result={query.data} meta={meta} overrides={overrides} onOverridesChange={onOverridesChange} />
    </div>
  );
}

/**
 * A stored session's analysis: a tab for each axis, the recommendation and charts for the one
 * chosen, the fit settings and the details of the recording. The session page and the results
 * of a test show the same thing.
 */
export function SessionResults({ id }: { id: string }) {
  const session = useSession(id);
  if (session.isPending) return <p className="text-sm text-muted-foreground">Loading session…</p>;
  if (session.isError) {
    return (
      <p role="alert" className="text-sm text-destructive">
        {session.error.message}
      </p>
    );
  }
  return <Loaded id={id} meta={session.data.meta} />;
}

function Loaded({ id, meta }: { id: string; meta: SessionMeta }) {
  const [selected, setSelected] = useState<Axis | null>(null);
  // The settings the user applied. Until they do, the fit starts from what the session recorded.
  const [applied, setApplied] = useState<FitParams | null>(null);
  const [overrides, setOverrides] = useState<string[]>([]);

  const axes = [...new Set(meta.segments.map((s) => s.axis))];
  const axis = selected && axes.includes(selected) ? selected : (axes[0] ?? null);
  const baseline = baselineFor(meta, axis ?? "X");
  const params = applied ?? baseline.params;
  const query = useSessionAnalysis({ sessionId: id, meta, axis, params, overrides });

  return (
    <div className="space-y-4">
      {axis === null ? (
        <p className="text-sm">This session has no recordings to analyse.</p>
      ) : (
        <>
          <Tabs value={axis} onValueChange={(value) => setSelected(value as Axis)}>
            <TabsList variant="line">
              {axes.map((a) => (
                <TabsTrigger key={a} value={a} className="after:bg-primary">
                  Axis {a}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
          <AnalysisView
            query={query}
            id={id}
            axis={axis}
            meta={meta}
            params={params}
            overrides={overrides}
            onOverridesChange={setOverrides}
          />
          <FitSettingsForm
            key={applied ? "applied" : axis}
            baseline={baseline}
            applied={applied}
            onApply={setApplied}
            onReset={() => setApplied(null)}
          />
        </>
      )}
      <MetaPanel meta={meta} analysis={query.data?.analysis ?? null} />
    </div>
  );
}
