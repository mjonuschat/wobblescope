import { type ReactNode, useState } from "react";

import type { AnalysisResult } from "@/analysis/protocol";
import { autoscale } from "@/components/charts/options";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { chipLabel } from "@/lib/format";
import type { Axis } from "@/lib/generated/Axis";
import type { ChipAnalysis } from "@/lib/generated/ChipAnalysis";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import { subheadingClass } from "@/ui-kit/CardLabel";
import { Panel } from "@/ui-kit/Panel";

import { ChipResult } from "./ChipResult";
import { CombinedResult } from "./CombinedResult";
import { EligibilityPanel } from "./EligibilityPanel";
import { pairSnippet } from "./pairSnippet";
import { RecommendationCard, SnippetBlock } from "./RecommendationCard";
import { effectiveSettings } from "./fit";
import { type SpectrogramSource, SpectrogramPanel } from "./SpectrogramPanel";

const ALL_CHIPS = "*all-chips*";

export interface PairColumn {
  axis: Axis;
  result: AnalysisResult | null;
  placeholder?: ReactNode;
  source: SpectrogramSource;
}

interface Props {
  columns: PairColumn[];
  meta: SessionMeta;
  overrides: string[];
  onOverridesChange?: (overrides: string[]) => void;
  readOnly?: boolean;
}

/**
 * The page lays the sections out one row each, X then Y. Cells carry `order` so that the grid puts
 * X and Y of a section side by side when it is wide, and X directly above Y when it is narrow.
 */
const rowOrder = (row: number, column: number) => row * 2 + column;

function Cell({ order, axis, children }: { order: number; axis: Axis; children: ReactNode }) {
  if (children === null || children === undefined || children === false) return null;
  // X always takes the left column and Y the right, so a section one axis lacks leaves its cell empty.
  const column = axis === "X" ? "@min-[1200px]:col-start-1" : "@min-[1200px]:col-start-2";
  return (
    <div className={`min-w-0 space-y-2 ${column}`} style={{ order }}>
      <p className={subheadingClass}>Axis {axis}</p>
      {children}
    </div>
  );
}

/** The analysis of the chip called `name`, or `null` for an axis that has no such chip. */
function chipOf(result: AnalysisResult | null, name: string | null): { chip: ChipAnalysis; index: number } | null {
  const chips = result?.analysis.per_chip ?? [];
  const index = chips.findIndex((c) => c.chip === name);
  return index >= 0 ? { chip: chips[index], index } : null;
}

export function AxisPairResults({ columns: given, meta, overrides, onOverridesChange, readOnly }: Props) {
  // X is always the first column, whatever order the session recorded the axes in.
  const columns = [...given].sort((a, b) => a.axis.localeCompare(b.axis));
  const [chipName, setChipName] = useState<string | null>(null);
  const [userScale, setUserScale] = useState<number | null>(null);
  const ready = columns.map((c) => c.result).filter((r): r is AnalysisResult => r !== null);
  const maxFreq = Math.max(200, ...ready.map((r) => effectiveSettings(r.analysis)?.max_freq ?? 0));

  const chipNames = [...new Set(ready.flatMap((r) => r.analysis.per_chip.map((c) => c.chip)))];
  const showAllTab = ready.some((r) => r.analysis.summed !== null && r.analysis.per_chip.length >= 2);
  const showAll = showAllTab && chipName === ALL_CHIPS;
  const activeName = showAll
    ? ALL_CHIPS
    : chipName !== null && chipNames.includes(chipName)
      ? chipName
      : (chipNames[0] ?? null);
  const shown = columns.map((c) => chipOf(c.result, showAll ? null : activeName));

  const autoScales = shown
    .map((s) => (s ? autoscale(s.chip, maxFreq) : undefined))
    .filter((v): v is number => v !== undefined);
  const scale = {
    value: userScale,
    onChange: setUserScale,
    auto: autoScales.length > 0 ? Math.max(...autoScales) : undefined,
  };

  // The all-chips charts share their vibration axis too.
  const combinedMax = Math.max(
    0,
    ...ready.flatMap((r) =>
      r.analysis.summed
        ? r.analysis.summed.psd.filter((_, i) => (r.analysis.summed?.freq_bins[i] ?? 0) <= maxFreq)
        : [],
    ),
  );
  const combinedScale = combinedMax > 0 ? combinedMax * 1.05 : undefined;

  const snippet = pairSnippet(
    columns[0].result?.snippets.worstCase ?? null,
    columns[1].result?.snippets.worstCase ?? null,
  );

  const chipTabs = chipNames.length > 0 && (
    <Tabs value={activeName ?? ""} onValueChange={setChipName}>
      <TabsList variant="line">
        {chipNames.map((name) => (
          <TabsTrigger key={name} value={name} className="after:bg-primary">
            {chipLabel(name)}
          </TabsTrigger>
        ))}
        {showAllTab && (
          <TabsTrigger value={ALL_CHIPS} className="after:bg-primary">
            All chips
          </TabsTrigger>
        )}
      </TabsList>
    </Tabs>
  );

  return (
    <div className="@container">
      <div className="grid grid-cols-1 gap-4 @min-[1200px]:grid-cols-2">
        {columns.map((column, i) => {
          const { axis, result, placeholder } = column;
          if (!result) {
            return (
              <Cell key={axis} order={rowOrder(0, i)} axis={axis}>
                {placeholder}
              </Cell>
            );
          }
          const { analysis, snippets } = result;
          const chosen = shown[i];
          return [
            <Cell key={`${axis}-recommendation`} order={rowOrder(0, i)} axis={axis}>
              {analysis.worst_case ? (
                <RecommendationCard
                  title="Recommended input shaper"
                  recommendation={analysis.worst_case}
                  snippet={snippets.worstCase}
                  performanceSnippet={snippets.worstCasePerformance}
                  datasetVibrations={analysis.worst_case_vibrations}
                  probePoints={meta.probe_points}
                />
              ) : (
                <Panel title="Recommendation">
                  <p role="status" className="text-sm">
                    No recommendation for this axis: none of its recordings could be used. The reasons are listed below.
                  </p>
                </Panel>
              )}
            </Cell>,
            <Cell key={`${axis}-quality`} order={rowOrder(2, i)} axis={axis}>
              <EligibilityPanel
                analysis={analysis}
                probePoints={meta.probe_points}
                overrides={overrides}
                onOverridesChange={onOverridesChange}
                readOnly={readOnly}
              />
            </Cell>,
            <Cell key={`${axis}-resonance`} order={rowOrder(4, i)} axis={axis}>
              {showAll && analysis.summed ? (
                <CombinedResult summed={analysis.summed} maxFreq={maxFreq} switcher={null} max={combinedScale} />
              ) : (
                chosen && (
                  <ChipResult
                    chip={chosen.chip}
                    switcher={null}
                    snippets={snippets.perChip[chosen.index]}
                    maxFreq={maxFreq}
                    decisions={analysis.eligibility.decisions}
                    scale={scale}
                  />
                )
              )}
            </Cell>,
            <Cell key={`${axis}-spectrogram`} order={rowOrder(5, i)} axis={axis}>
              {!showAll && chosen && (
                <SpectrogramPanel
                  key={chosen.chip.chip}
                  chip={chosen.chip}
                  meta={meta}
                  source={column.source}
                  maxFreq={maxFreq}
                />
              )}
            </Cell>,
          ];
        })}
        {snippet && (
          <div className="min-w-0 @min-[1200px]:col-span-2" style={{ order: rowOrder(1, 0) }}>
            <Panel title="Config for both axes">
              <SnippetBlock text={snippet} />
            </Panel>
          </div>
        )}
        {chipTabs && (
          <div className="min-w-0 @min-[1200px]:col-span-2" style={{ order: rowOrder(3, 0) }}>
            {chipTabs}
          </div>
        )}
      </div>
    </div>
  );
}
