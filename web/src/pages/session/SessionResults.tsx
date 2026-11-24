import { type ReactNode, useState } from "react";

import { useSessionAnalysis } from "@/analysis/useSessionAnalysis";
import { Panel } from "@/ui-kit/Panel";
import { axisTabClass, axisTabsListClass } from "@/ui-kit/axisTabs";
import { useResultsLayout } from "@/ui-kit/resultsLayout";
import { Waiting } from "@/components/Waiting";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useSession } from "@/lib/api/queries";
import type { Axis } from "@/lib/generated/Axis";
import type { FitParams } from "@/lib/generated/FitParams";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import { AxesMapResults } from "./AxesMapResults";
import { AxisPairResults, type PairColumn } from "./AxisPairResults";
import { AxisResults } from "./AxisResults";
import { FitSettingsForm } from "./FitSettingsForm";
import { MetaPanel } from "./MetaPanel";
import { BeltResults } from "./belts/BeltResults";
import { UnsupportedDirections } from "./belts/UnsupportedDirections";
import { VibrationsResults } from "./vibrations/VibrationsResults";
import { SaveResultButton } from "./SaveResultDialog";
import { sessionAxes } from "./axes";
import { baselineFor, baselineForAxes } from "./fit";

type AnalysisQuery = ReturnType<typeof useSessionAnalysis>;

function analysisPlaceholder(query: AnalysisQuery, axis: Axis): ReactNode {
  if (query.isPending) {
    const { progress } = query;
    const message =
      progress.phase === "queued"
        ? "Waiting for the previous analysis…"
        : progress.phase === "downloading"
          ? `Downloading samples for axis ${axis}…`
          : `Analysing axis ${axis}…`;
    const download = progress.phase === "downloading" ? { loaded: progress.loaded, total: progress.total } : undefined;
    return <Waiting download={download}>{message}</Waiting>;
  }
  if (query.isError) {
    return (
      <p role="alert" className="rounded-lg border border-destructive p-4 text-sm text-destructive">
        The analysis failed: {query.error.message}
      </p>
    );
  }
  return null;
}

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
  const placeholder = analysisPlaceholder(query, axis);
  if (placeholder || !query.data) return placeholder;
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
  const { meta, summary } = session.data;
  switch (summary.kind) {
    case "belts":
      return <BeltResults id={id} meta={meta} header={header} />;
    case "axesmap":
      return (
        <div className="space-y-4">
          <AxesMapResults id={id} meta={meta} header={header} />
          <MetaPanel meta={meta} />
        </div>
      );
    case "vibrations":
      return (
        <div className="space-y-4">
          <VibrationsResults id={id} meta={meta} header={header} />
          <MetaPanel meta={meta} />
        </div>
      );
    case "other":
      return (
        <div className="space-y-4">
          {header(null)}
          <UnsupportedDirections meta={meta} />
        </div>
      );
    case "axes":
      return <Loaded id={id} meta={meta} header={header} />;
  }
}

/**
 * What shows above the analysis. It gets the Save result button, or null while there is nothing
 * to save yet, and the belt similarity rows, or null when there are none, and places them where
 * the page wants them.
 */
export type Header = (save: ReactNode, summary?: ReactNode) => ReactNode;

const defaultHeader: Header = (save, summary) =>
  summary ? (
    <Panel title="Result" action={save}>
      {summary}
    </Panel>
  ) : save ? (
    <div className="flex justify-end">{save}</div>
  ) : null;

function Loaded({ id, meta, header }: { id: string; meta: SessionMeta; header: Header }) {
  const layout = useResultsLayout();
  const [selected, setSelected] = useState<Axis | null>(null);
  // The settings the user applied. Until they do, the fit starts from what the session recorded.
  const [applied, setApplied] = useState<FitParams | null>(null);
  const [overrides, setOverrides] = useState<string[]>([]);

  // Only a session of X and Y segments gets here, so no vector is dropped.
  const axes = sessionAxes(meta);
  const axis = selected && axes.includes(selected) ? selected : (axes[0] ?? null);
  const together = layout === "side-by-side" && axes.length >= 2;
  const baseline = together ? baselineForAxes(meta, axes) : baselineFor(meta, axis ?? "X");
  const params = applied ?? baseline.params;
  // Every axis is analysed, so that a save keeps both and a switch of tab shows at once.
  const queries = [
    useSessionAnalysis({ sessionId: id, meta, axis: axes[0] ?? null, params, overrides }),
    useSessionAnalysis({ sessionId: id, meta, axis: axes[1] ?? null, params, overrides }),
  ];
  const queryOf = (a: Axis) => queries[Math.max(0, axes.indexOf(a))];
  const query = queryOf(axis ?? "X");

  const save =
    axis === null ? null : (
      <SaveResultButton
        sessionId={id}
        meta={meta}
        params={params}
        overrides={overrides}
        axes={axes.map((a) => ({ axis: a, result: queryOf(a).data ?? null, failed: queryOf(a).isError }))}
      />
    );
  const column = (a: Axis): PairColumn => ({
    axis: a,
    result: queryOf(a).data ?? null,
    placeholder: analysisPlaceholder(queryOf(a), a),
    source: { sessionId: id, axis: a },
  });

  return (
    <div className="space-y-4">
      {header(save)}
      {axis === null ? (
        <p className="text-sm">This session has no recordings to analyse.</p>
      ) : (
        <>
          {together ? (
            <AxisPairResults
              columns={[column(axes[0]), column(axes[1])]}
              meta={meta}
              overrides={overrides}
              onOverridesChange={setOverrides}
            />
          ) : (
            <>
              <Tabs value={axis} onValueChange={(value) => setSelected(value as Axis)}>
                <TabsList variant="line" className={axisTabsListClass}>
                  {axes.map((a) => (
                    <TabsTrigger key={a} value={a} className={axisTabClass}>
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
            </>
          )}
          <FitSettingsForm
            key={applied ? "applied" : together ? "both" : axis}
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
