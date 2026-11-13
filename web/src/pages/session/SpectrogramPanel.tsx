import { useState } from "react";

import { useSpectrogram } from "@/analysis/useSpectrogram";
import { Panel } from "@/components/Panel";
import { EChart } from "@/components/charts/EChart";
import { pointLabel, spectrogramOption } from "@/components/charts/options";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { Axis } from "@/lib/generated/Axis";
import type { ChipAnalysis } from "@/lib/generated/ChipAnalysis";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

export interface SpectrogramSource {
  sessionId: string;
  /** A saved result to fetch the samples from; `sessionId` is then only the cache key. */
  resultId?: string;
  axis: Axis;
}

/**
 * The probe points the axis has a recording of `chip` at, read from the metadata. The analysis
 * leaves out a recording that failed to load, but its map can still be asked for, to say why.
 */
export function pointsOf(meta: SessionMeta, axis: Axis, chip: string): number[] {
  const position = meta.chips.findIndex((c) => c.name === chip);
  const points = meta.segments
    .filter((s) => s.axis === axis && s.files.some((f) => f.chip === position))
    .map((s) => s.point);
  return [...new Set(points)].sort((a, b) => a - b);
}

interface Props {
  chip: ChipAnalysis;
  meta: SessionMeta;
  source: SpectrogramSource;
  /** The map keeps the frequencies up to this, in Hz. */
  maxFreq: number;
}

/** The time-frequency map of one chip's recording. It is made when the panel is shown and cached by recording. */
export function SpectrogramPanel({ chip, meta, source, maxFreq }: Props) {
  const points = pointsOf(meta, source.axis, chip.chip);
  const [chosen, setChosen] = useState<number | null>(null);
  const point = chosen !== null && points.includes(chosen) ? chosen : (points[0] ?? null);
  const query = useSpectrogram({
    sessionId: source.sessionId,
    resultId: source.resultId,
    meta,
    axis: source.axis,
    chip: chip.chip,
    point,
    maxFreq,
    enabled: true,
  });

  if (point === null) {
    return (
      <Panel title="Spectrogram">
        <p className="text-sm text-muted-foreground">This chip has no recording on this axis.</p>
      </Panel>
    );
  }
  const selector =
    points.length > 1 ? (
      <ToggleGroup
        variant="outline"
        size="sm"
        aria-label="Probe point"
        value={[String(point)]}
        onValueChange={(next) => {
          if (next.length > 0) setChosen(Number(next[0]));
        }}
      >
        {points.map((p) => (
          <ToggleGroupItem key={p} value={String(p)}>
            {pointLabel(p, meta.probe_points)}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    ) : null;
  const peaks = chip.peaks.find(([p]) => p === point)?.[1] ?? null;
  return (
    <Panel title="Spectrogram" action={selector}>
      {query.isPending && (
        <p role="status" className="text-sm text-muted-foreground">
          Making the spectrogram…
        </p>
      )}
      {query.isError && (
        <p role="alert" className="rounded-lg border border-destructive p-4 text-sm text-destructive">
          The spectrogram could not be made: {query.error.message}
        </p>
      )}
      {query.isSuccess && (
        <EChart
          option={spectrogramOption(query.data, peaks, maxFreq)}
          label={`Spectrogram of ${chip.chip}${points.length > 1 ? ` at ${pointLabel(point, meta.probe_points)}` : ""}`}
          height={420}
        />
      )}
    </Panel>
  );
}
