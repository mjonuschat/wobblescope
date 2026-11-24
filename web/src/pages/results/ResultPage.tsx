import { Link } from "@tanstack/react-router";
import { useState } from "react";

import type { AnalysisResult } from "@/analysis/protocol";
import { useSessionAnalysis } from "@/analysis/useSessionAnalysis";
import { ArrowLeftIcon } from "lucide-react";

import { DownloadProgress } from "@/components/DownloadProgress";
import { axisTabClass, axisTabsListClass } from "@/ui-kit/axisTabs";
import { Waiting } from "@/components/Waiting";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useResult } from "@/lib/api/queries";
import { formatTime } from "@/lib/format";
import type { Axis } from "@/lib/generated/Axis";
import type { ResultDetail } from "@/lib/generated/ResultDetail";

import { AxisPairResults } from "../session/AxisPairResults";
import { AxisResults } from "../session/AxisResults";
import { MetaPanel } from "../session/MetaPanel";
import { compareAnalyses } from "./compare";
import { Alert } from "@/ui-kit/Alert";
import { PageHeader } from "@/ui-kit/PageHeader";
import { useResultsLayout } from "@/ui-kit/resultsLayout";

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
    const { progress } = fresh;
    const message =
      progress.phase === "queued"
        ? "Waiting for the previous analysis…"
        : progress.phase === "downloading"
          ? "Downloading the saved samples…"
          : "Checking the saved data with a fresh analysis…";
    return (
      <div className="space-y-2">
        <p className="text-sm text-muted-foreground">{message}</p>
        {progress.phase === "downloading" && <DownloadProgress loaded={progress.loaded} total={progress.total} />}
      </div>
    );
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
        <Alert tone="warning">
          This result was saved with core {record.core_version} and re-analysed with core{" "}
          {fresh.data.analysis.core_version}. Differences below may come from the version.
        </Alert>
      )}
      {sameAnswer ? (
        <p className="text-sm text-muted-foreground">
          The saved data is intact: a fresh analysis gives the same answer.
        </p>
      ) : (
        <Alert tone="warning" role="alert" title="A fresh analysis of the saved data gives a different answer:">
          <ul className="list-disc pl-5">
            {differences.map((d) => (
              <li key={d}>
                {axis.axis} {d}
              </li>
            ))}
          </ul>
        </Alert>
      )}
    </div>
  );
}

function resultOf(axis: AxisSnapshot): AnalysisResult {
  return {
    analysis: axis.analysis,
    snippets: {
      worstCase: axis.snippets.worst_case,
      worstCasePerformance: axis.snippets.worst_case_performance,
      perChip: axis.snippets.per_chip,
    },
  };
}

function PairView({ rid, detail, axes }: { rid: string; detail: ResultDetail; axes: [AxisSnapshot, AxisSnapshot] }) {
  return (
    <div className="space-y-4">
      {axes.map((axis) => (
        <Comparison key={axis.axis} rid={rid} detail={detail} axis={axis} />
      ))}
      <AxisPairResults
        columns={[
          {
            axis: axes[0].axis,
            result: resultOf(axes[0]),
            source: { sessionId: `result:${rid}`, resultId: rid, axis: axes[0].axis },
          },
          {
            axis: axes[1].axis,
            result: resultOf(axes[1]),
            source: { sessionId: `result:${rid}`, resultId: rid, axis: axes[1].axis },
          },
        ]}
        meta={detail.meta}
        overrides={detail.record.overrides}
        readOnly
      />
    </div>
  );
}

function AxisView({ rid, detail, axis }: { rid: string; detail: ResultDetail; axis: AxisSnapshot }) {
  const { record, meta } = detail;
  const result = resultOf(axis);
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
  const layout = useResultsLayout();

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
  const pair =
    layout === "side-by-side" && record.axes.length >= 2
      ? ([record.axes[0], record.axes[1]] as [AxisSnapshot, AxisSnapshot])
      : null;

  return (
    <div className={`mx-auto space-y-6 ${pair ? "max-w-[1600px]" : "max-w-5xl"}`}>
      <PageHeader
        title={record.label ?? record.id}
        leading={
          <Button render={<Link to="/results" />} variant="outline" size="icon" aria-label="Back to results">
            <ArrowLeftIcon />
          </Button>
        }
      />
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
      ) : pair ? (
        <PairView rid={rid} detail={detail} axes={pair} />
      ) : (
        <>
          <Tabs value={axis.axis} onValueChange={(value) => setSelected(value as Axis)}>
            <TabsList variant="line" className={axisTabsListClass}>
              {axes.map((a) => (
                <TabsTrigger key={a} value={a} className={axisTabClass}>
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
