import type { EChartsCoreOption } from "echarts/core";
import { useId } from "react";

import { EXPANDED_CHART_HEIGHT, ExpandDialog } from "@/components/ExpandDialog";
import { EChart } from "@/components/charts/EChart";
import { WIDE_CHART } from "@/components/charts/size";
import { InfoHint } from "@/components/InfoHint";
import { cn } from "@/lib/utils";
import { DEFAULT_CHART_THEME, type ChartTheme, useChartTheme } from "@/ui-kit/chartTheme";
import type { AxesMapResult } from "@/lib/generated/AxesMapResult";
import type { AxisMatch } from "@/lib/generated/AxisMatch";
import type { ChipAxis } from "@/lib/generated/ChipAxis";

/**
 * The colors of the chip axes x, y and z: violet, the accent and magenta, the hues of Shake&Tune's purple,
 * orange and pink, taken from the theme so each reads on a light and a dark surface (D47).
 */
export const chipAxisColors = (theme: ChartTheme): readonly [string, string, string] => [
  theme.series[0],
  theme.accent,
  theme.series[2],
];

const MACHINE_AXES = ["X", "Y", "Z"] as const;
const CHIP_AXIS_INDEX: Record<ChipAxis, number> = { X: 0, Y: 1, Z: 2 };

/** A chip axis as the map writes it, with its sign: `-z`. */
export function chipAxisText(match: AxisMatch): string {
  return match.chip_axis === null ? "" : `${match.negative ? "-" : ""}${match.chip_axis.toLowerCase()}`;
}

export interface MovementZone {
  axis: (typeof MACHINE_AXES)[number];
  /** Where the zone starts and ends on the joined time axis, in seconds. */
  start: number;
  end: number;
  virtual: boolean;
  /** The line under the watermark letter. */
  note: string;
  /** The detected peak, at its time on the joined axis, in mm/s. */
  peak: { chipAxis: number; time: number; value: number; label: string } | null;
}

/** The three segments joined end to end, each segment's time starting where the last one ended. */
export function movementZones(result: AxesMapResult): MovementZone[] {
  let offset = 0;
  return result.curves.map((curve, i) => {
    const match = result.axes[i];
    const start = offset;
    const end = start + (curve.t.length > 0 ? curve.t[curve.t.length - 1] - curve.t[0] : 0);
    offset = end;
    const virtual = match.state === "virtual";
    const note = virtual
      ? "Inferred (no signal on this axis)"
      : match.state === "measured"
        ? `Confidence: ${Math.round(match.confidence * 100)}%`
        : "Not determined";
    const peak =
      match.state === "measured" && match.chip_axis !== null
        ? {
            chipAxis: CHIP_AXIS_INDEX[match.chip_axis],
            time: start + match.peak_time_s,
            value: match.peak_velocity,
            label: `${match.peak_velocity.toFixed(0)} mm/s`,
          }
        : null;
    return { axis: MACHINE_AXES[i], start, end, virtual, note, peak };
  });
}

/**
 * The movement sequence: the drift-corrected velocity of the chip axes over the three segments,
 * with a watermark letter and confidence per zone, the zones split by dashed lines and each
 * detected peak marked with its value.
 */
export function movementOption(result: AxesMapResult, theme: ChartTheme = DEFAULT_CHART_THEME): EChartsCoreOption {
  const zones = movementZones(result);
  const series = chipAxisColors(theme).map((color, k) => {
    const line: Record<string, unknown> = {
      name: `Chip ${"xyz"[k]}`,
      type: "line",
      showSymbol: false,
      color,
      lineStyle: { color, width: 1.2 },
      data: result.curves.flatMap((curve, i) =>
        curve.t.map((t, n) => [zones[i].start + t - curve.t[0], curve.velocity[k][n]]),
      ),
    };
    const peaks = zones.flatMap((z) => (z.peak?.chipAxis === k ? [z.peak] : []));
    if (peaks.length > 0) {
      line.markPoint = {
        silent: true,
        symbol: "circle",
        symbolSize: 9,
        itemStyle: { color, borderColor: theme.surface, borderWidth: 0.5 },
        data: peaks.map((p) => ({
          coord: [p.time, p.value],
          label: {
            formatter: p.label,
            position: p.value > 0 ? "top" : "bottom",
            color,
            fontSize: 11,
            fontWeight: "bold",
          },
        })),
      };
    }
    return line;
  });
  series[0].markLine = {
    silent: true,
    symbol: "none",
    label: { show: false },
    lineStyle: { type: "dashed", color: theme.muted, width: 2, opacity: 0.6 },
    data: zones.slice(1).map((z) => ({ xAxis: z.start })),
  };
  series[0].markArea = {
    silent: true,
    data: zones.map((z) => [
      {
        xAxis: z.start,
        itemStyle: { color: z.virtual ? theme.alpha(theme.muted, 0.15) : "transparent" },
        label: {
          position: "inside",
          formatter: `{letter|${z.axis}}\n{note|${z.note.replace(" (", "\n(")}}`,
          rich: {
            letter: {
              fontSize: 56,
              fontWeight: "bold",
              color: z.virtual ? theme.alpha(theme.muted, 0.45) : theme.alpha(theme.text, 0.3),
            },
            note: {
              fontSize: 11,
              fontWeight: z.virtual ? "normal" : "bold",
              color: z.virtual ? theme.alpha(theme.muted, 0.45) : theme.alpha(theme.text, 0.3),
            },
          },
        },
      },
      { xAxis: z.end },
    ]),
  };
  const total = zones[zones.length - 1].end;
  return {
    backgroundColor: "transparent",
    textStyle: { color: theme.text },
    tooltip: { trigger: "axis", valueFormatter: (v: number) => `${v.toFixed(1)} mm/s` },
    legend: {
      type: "scroll",
      data: series.map((s) => s.name as string),
      top: 0,
      left: 56,
      textStyle: { color: theme.text },
    },
    grid: { top: 52, left: 76, right: 24, bottom: 40 },
    xAxis: {
      type: "value",
      name: "Time (s)",
      nameLocation: "middle",
      nameGap: 26,
      min: 0,
      max: total,
      axisLabel: { showMaxLabel: false },
      splitLine: { lineStyle: { color: theme.grid } },
    },
    yAxis: {
      type: "value",
      name: "Velocity (mm/s)",
      nameLocation: "middle",
      nameRotate: 90,
      nameGap: 46,
      // Shake&Tune pads the range by 15 % at each end, so the peak labels clear the edge.
      min: (extent: { min: number; max: number }) => extent.min - padding(extent),
      max: (extent: { min: number; max: number }) => extent.max + padding(extent),
      // The padded ends are not round numbers, so only the ticks inside the range are labelled.
      axisLabel: { showMinLabel: false, showMaxLabel: false },
      splitLine: { lineStyle: { color: theme.grid } },
    },
    series,
  };
}

/**
 * An isometric view from the (+X, -Y, +Z) octant, in SVG coordinates (y grows downwards): X
 * runs to the lower right, Y to the upper right and Z straight up.
 */
export function isometric([x, y, z]: readonly number[]): [number, number] {
  return [(x + y) / Math.SQRT2, (x - y - 2 * z) / Math.sqrt(6)];
}

export interface OrientationArrow {
  label: string;
  color: string;
  end: [number, number];
  labelAt: [number, number];
}

/**
 * The dashed machine axes and the chip axes of the orientation view. Column `i` of the rotation
 * is where chip axis `i` points in machine coordinates. Null when there is no rotation.
 */
export function orientationArrows(
  result: AxesMapResult,
  theme: ChartTheme = DEFAULT_CHART_THEME,
): { machine: OrientationArrow[]; chip: OrientationArrow[] } | null {
  const rotation = result.rotation;
  if (rotation === null) return null;
  const scaled = (v: number[], s: number) => isometric(v.map((c) => c * s));
  const machine = MACHINE_AXES.map((label, i) => {
    const unit = [0, 0, 0];
    unit[i] = 1;
    return { label, color: theme.muted, end: scaled(unit, 1.15), labelAt: scaled(unit, 1.3) };
  });
  const chip = chipAxisColors(theme).map((color, i) => {
    const column = [rotation[0][i], rotation[1][i], rotation[2][i]];
    return {
      label: "XYZ"[i],
      color,
      end: scaled(column, 0.9),
      labelAt: besideArrow(scaled(column, 0.9)),
    };
  });
  return { machine, chip };
}

/**
 * A label position beside an arrow, to its left, near the tip. Chip axes often lie along the
 * machine axes, so a label past the tip would sit on the machine arrow and its label.
 */
function besideArrow(end: [number, number]): [number, number] {
  const length = Math.hypot(end[0], end[1]) || 1;
  const [dx, dy] = [end[0] / length, end[1] / length];
  const side = dy > 0 ? 1 : -1;
  const [px, py] = [-dy * side, dx * side];
  return [end[0] * 0.8 + px * 0.12, end[1] * 0.8 + py * 0.12];
}

function Arrow({
  arrow,
  dashed,
  marker,
  width,
}: {
  arrow: OrientationArrow;
  dashed?: boolean;
  marker: string;
  width: number;
}) {
  return (
    <line
      x1={0}
      y1={0}
      x2={arrow.end[0]}
      y2={arrow.end[1]}
      stroke={arrow.color}
      strokeWidth={width}
      strokeDasharray={dashed ? "0.08 0.05" : undefined}
      opacity={dashed ? 0.6 : 1}
      markerEnd={`url(#${marker})`}
    />
  );
}

function OrientationView({ result }: { result: AxesMapResult }) {
  const id = useId();
  const theme = useChartTheme();
  const arrows = orientationArrows(result, theme);
  if (arrows === null) return null;
  const marker = (i: number | "machine") => `${id}-head-${i}`;
  const heads = [
    { key: marker("machine"), color: theme.muted },
    ...chipAxisColors(theme).map((color, i) => ({ key: marker(i), color })),
  ];
  return (
    <svg viewBox="-1.5 -1.5 3 3" role="img" aria-label="Chip orientation" className="aspect-square w-full max-w-80">
      <defs>
        {heads.map((h) => (
          <marker
            key={h.key}
            id={h.key}
            viewBox="0 0 10 10"
            refX="8"
            refY="5"
            markerWidth="5"
            markerHeight="5"
            orient="auto"
          >
            <path d="M0,0 L10,5 L0,10 z" fill={h.color} />
          </marker>
        ))}
      </defs>
      {arrows.machine.map((a) => (
        <g key={a.label}>
          <Arrow arrow={a} dashed marker={marker("machine")} width={0.03} />
          <text
            x={a.labelAt[0]}
            y={a.labelAt[1]}
            fill={theme.muted}
            fontSize={0.14}
            fontWeight="bold"
            textAnchor="middle"
            dominantBaseline="middle"
          >
            {a.label}
          </text>
        </g>
      ))}
      {arrows.chip.map((a, i) => (
        <g key={a.label}>
          <Arrow arrow={a} marker={marker(i)} width={0.04} />
          <text
            x={a.labelAt[0]}
            y={a.labelAt[1]}
            fill={a.color}
            fontSize={0.18}
            fontWeight="bold"
            textAnchor="end"
            dominantBaseline="middle"
          >
            {a.label}
          </text>
        </g>
      ))}
    </svg>
  );
}

function MovementChart({ result, height }: { result: AxesMapResult; height?: number | string }) {
  const theme = useChartTheme();
  return (
    <EChart
      option={movementOption(result, theme)}
      label={`Movement sequence of ${result.chip}`}
      height={height}
      minWidth={WIDE_CHART}
    />
  );
}

/** The padding at each end of the velocity axis: 15 % of the data range, or 1 mm/s for a flat recording. */
function padding({ min, max }: { min: number; max: number }): number {
  return max > min ? (max - min) * 0.15 : 1;
}

const GRAPH_TITLE = "text-sm font-medium";

export function AxesMapPlot({ result }: { result: AxesMapResult }) {
  return (
    <div className={cn("grid gap-4", result.rotation !== null && "lg:grid-cols-[5fr_3fr]")}>
      <section aria-labelledby="movement-title" className="space-y-1">
        <div className="flex items-center justify-between gap-2">
          <h3 id="movement-title" className={GRAPH_TITLE}>
            Movement sequence
          </h3>
          <ExpandDialog title={`Movement sequence: ${result.chip}`}>
            <MovementChart result={result} height={EXPANDED_CHART_HEIGHT} />
          </ExpandDialog>
        </div>
        <MovementChart result={result} />
      </section>
      {result.rotation !== null && (
        <section aria-labelledby="orientation-title" className="space-y-1">
          <div className="flex h-8 items-center gap-1">
            <h3 id="orientation-title" className={GRAPH_TITLE}>
              Chip orientation
            </h3>
            <InfoHint label="Chip orientation">Dashed: printer axes. Coloured: chip axes, as mounted.</InfoHint>
          </div>
          <div className="flex justify-center">
            <OrientationView result={result} />
          </div>
        </section>
      )}
    </div>
  );
}
