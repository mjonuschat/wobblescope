import { Link } from "@tanstack/react-router";
import { useState } from "react";

import type { AnalysisResult } from "@/analysis/protocol";
import { useSessionAnalysis } from "@/analysis/useSessionAnalysis";
import { ArrowLeftIcon } from "lucide-react";

import { Waiting } from "@/components/Waiting";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useResult } from "@/lib/api/queries";
import { formatTime } from "@/lib/format";
import type { Axis } from "@/lib/generated/Axis";
import type { ResultDetail } from "@/lib/generated/ResultDetail";

import { AxisResults } from "../session/AxisResults";
import { MetaPanel } from "../session/MetaPanel";
import { compareAnalyses } from "./compare";

type AxisSnapshot = ResultDetail["record"]["axes"][number];

/**
 * Runs the saved samples through the current core in the background and says whether the
 * saved answer still holds. It never changes what the page shows.
 */
function Comparison({ rid, detail, axis }: { rid: string; detail: ResultDetail; axis: AxisSnapshot }) {
  const { record, meta } = detail;
  const fresh = useSessionAnalysis({
    sessionId: `result:${rid}`,
    resultId: rid,
    meta,
    axis: axis.axis,
    params: record.fit_params,
    overrides: record.overrides,
  });
  if (fresh.isPending) {
    return <p className="text-sm text-muted-foreground">Checking the saved data with a fresh analysis…</p>;
  }
  if (fresh.isError) {
    return (
      <p className="text-sm text-muted-foreground">The saved data could not be re-analysed: {fresh.error.message}</p>
    );
  }
  const { sameAnswer, differences } = compareAnalyses(axis.analysis, fresh.data.analysis);
  const versionDiffers = record.core_version !== fresh.data.analysis.core_version;
  return (
    <div className="space-y-2">
      {versionDiffers && (
        <p role="status" className="rounded-lg border border-amber-500 p-3 text-sm text-amber-400">
          This result was saved with core {record.core_version} and re-analysed with core{" "}
          {fresh.data.analysis.core_version}. Differences below may come from the version.
        </p>
      )}
      {sameAnswer ? (
        <p className="text-sm text-muted-foreground">
          The saved data is intact: a fresh analysis gives the same answer.
        </p>
      ) : (
        <div role="alert" className="space-y-1 rounded-lg border border-amber-500 p-3 text-sm">
          <p className="text-amber-400">A fresh analysis of the saved data gives a different answer:</p>
          <ul className="list-disc pl-5">
            {differences.map((d) => (
              <li key={d}>
                {axis.axis} {d}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function AxisView({ rid, detail, axis }: { rid: string; detail: ResultDetail; axis: AxisSnapshot }) {
  const { record, meta } = detail;
  const result: AnalysisResult = {
    analysis: axis.analysis,
    snippets: {
      worstCase: axis.snippets.worst_case,
      worstCasePerformance: axis.snippets.worst_case_performance,
      perChip: axis.snippets.per_chip,
    },
  };
  return (
    <div className="space-y-4">
      <Comparison rid={rid} detail={detail} axis={axis} />
      <AxisResults
        result={result}
        meta={meta}
        overrides={record.overrides}
        source={{ sessionId: `result:${rid}`, resultId: rid, axis: axis.axis }}
        readOnly
      />
    </div>
  );
}

export function ResultPage({ rid }: { rid: string }) {
  const query = useResult(rid);
  const [selected, setSelected] = useState<Axis | null>(null);

  if (query.isPending) return <Waiting>Loading result…</Waiting>;
  if (query.isError) {
    return (
      <div className="space-y-2">
        <p role="alert" className="text-sm text-destructive">
          {query.error.message}
        </p>
        <Link to="/results" className="text-sm underline">
          Back to results
        </Link>
      </div>
    );
  }

  const detail = query.data;
  const { record, meta } = detail;
  const axes = record.axes.map((a) => a.axis);
  const axis = record.axes.find((a) => a.axis === selected) ?? record.axes[0];

  return (
    <div className="mx-auto max-w-5xl space-y-6">
      <div className="flex items-center gap-3">
        <Button render={<Link to="/results" />} variant="outline" size="icon" aria-label="Back to results">
          <ArrowLeftIcon />
        </Button>
        <h1 className="text-xl font-semibold">{record.label ?? record.id}</h1>
      </div>
      <p className="text-sm text-muted-foreground">
        Saved {formatTime(record.created_at)} from session{" "}
        <Link to="/sessions/$id" params={{ id: record.session_id }} className="underline">
          {record.session_id}
        </Link>
        , core {record.core_version}
      </p>
      <MetaPanel meta={meta} />
      {axis === undefined ? (
        <p className="text-sm">This result holds no analysed axis.</p>
      ) : (
        <>
          <Tabs value={axis.axis} onValueChange={(value) => setSelected(value as Axis)}>
            <TabsList variant="line">
              {axes.map((a) => (
                <TabsTrigger key={a} value={a} className="after:bg-primary">
                  Axis {a}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
          <AxisView key={axis.axis} rid={rid} detail={detail} axis={axis} />
        </>
      )}
    </div>
  );
}
