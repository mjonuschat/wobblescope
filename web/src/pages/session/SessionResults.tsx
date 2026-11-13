import { type ReactNode, useState } from "react";

import { useSessionAnalysis } from "@/analysis/useSessionAnalysis";
import { Waiting } from "@/components/Waiting";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useSession } from "@/lib/api/queries";
import type { Axis } from "@/lib/generated/Axis";
import type { FitParams } from "@/lib/generated/FitParams";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import { AxisResults } from "./AxisResults";
import { FitSettingsForm } from "./FitSettingsForm";
import { MetaPanel } from "./MetaPanel";
import { SaveResultButton } from "./SaveResultDialog";
import { baselineFor } from "./fit";

type AnalysisQuery = ReturnType<typeof useSessionAnalysis>;

function AnalysisView({
  sessionId,
  query,
  axis,
  meta,
  overrides,
  onOverridesChange,
}: {
  sessionId: string;
  query: AnalysisQuery;
  axis: Axis;
  meta: SessionMeta;
  overrides: string[];
  onOverridesChange: (overrides: string[]) => void;
}) {
  if (query.isPending) {
    return <Waiting>Analysing axis {axis}…</Waiting>;
  }
  if (query.isError) {
    return (
      <p role="alert" className="rounded-lg border border-destructive p-4 text-sm text-destructive">
        The analysis failed: {query.error.message}
      </p>
    );
  }
  return (
    <AxisResults
      result={query.data}
      meta={meta}
      overrides={overrides}
      source={{ sessionId, axis }}
      onOverridesChange={onOverridesChange}
    />
  );
}

export function SessionResults({ id, header = defaultHeader }: { id: string; header?: Header }) {
  const session = useSession(id);
  if (session.isPending) {
    return (
      <div className="space-y-4">
        {header(null)}
        <Waiting>Loading session…</Waiting>
      </div>
    );
  }
  if (session.isError) {
    return (
      <div className="space-y-4">
        {header(null)}
        <p role="alert" className="text-sm text-destructive">
          {session.error.message}
        </p>
      </div>
    );
  }
  return <Loaded id={id} meta={session.data.meta} header={header} />;
}

/**
 * What shows above the analysis. It gets the Save result button, or null while there is nothing
 * to save yet, and places it where the page wants it.
 */
export type Header = (save: ReactNode) => ReactNode;

const defaultHeader: Header = (save) => (save ? <div className="flex justify-end">{save}</div> : null);

function Loaded({ id, meta, header }: { id: string; meta: SessionMeta; header: Header }) {
  const [selected, setSelected] = useState<Axis | null>(null);
  // The settings the user applied. Until they do, the fit starts from what the session recorded.
  const [applied, setApplied] = useState<FitParams | null>(null);
  const [overrides, setOverrides] = useState<string[]>([]);

  const axes = [...new Set(meta.segments.map((s) => s.axis))];
  const axis = selected && axes.includes(selected) ? selected : (axes[0] ?? null);
  const baseline = baselineFor(meta, axis ?? "X");
  const params = applied ?? baseline.params;
  const query = useSessionAnalysis({ sessionId: id, meta, axis, params, overrides });

  const save =
    axis === null ? null : (
      <SaveResultButton
        sessionId={id}
        meta={meta}
        axis={axis}
        params={params}
        overrides={overrides}
        result={query.data ?? null}
      />
    );

  return (
    <div className="space-y-4">
      {header(save)}
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
            sessionId={id}
            query={query}
            axis={axis}
            meta={meta}
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
