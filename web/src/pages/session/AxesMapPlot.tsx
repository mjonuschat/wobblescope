import type { EChartsCoreOption } from "echarts/core";
import { useId } from "react";

import { EXPANDED_CHART_HEIGHT, ExpandDialog } from "@/components/ExpandDialog";
import { EChart } from "@/components/charts/EChart";
import type { AxesMapResult } from "@/lib/generated/AxesMapResult";
import type { AxisMatch } from "@/lib/generated/AxisMatch";
import type { ChipAxis } from "@/lib/generated/ChipAxis";
import type { NoiseVerdict } from "@/lib/generated/NoiseVerdict";

/** Shake&Tune's colours for the chip axes x, y and z. */
export const CHIP_AXIS_COLORS = ["#70088C", "#FF8D32", "#F2055C"] as const;

const MACHINE_AXES = ["X", "Y", "Z"] as const;
const CHIP_AXIS_INDEX: Record<ChipAxis, number> = { X: 0, Y: 1, Z: 2 };
const TEXT = "#d4d4d4";
const MUTED = "#a3a3a3";
const GRID = "rgba(255, 255, 255, 0.08)";
const WATERMARK = "rgba(212, 212, 212, 0.3)";
const VIRTUAL_ZONE = "rgba(128, 128, 128, 0.15)";
const VIRTUAL_TEXT = "rgba(160, 160, 160, 0.45)";
const NOISE_WORDS: Record<NoiseVerdict, string> = { Fine: "OK", High: "High", TooHigh: "Too high" };

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
      ? "Extrapolated (no signal on this axis)"
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

export function gravityText(result: AxesMapResult): string {
  return `Measured gravity: ${result.gravity.toFixed(3)} m/s²`;
}

export function noiseText(result: AxesMapResult): string {
  return `Noise: ${result.noise.toFixed(0)} mm/s² (${NOISE_WORDS[result.noise_verdict]})`;
}

/**
 * The movement sequence: the drift-corrected velocity of the chip axes over the three segments,
 * with a watermark letter and confidence per zone, the zones split by dashed lines and each
 * detected peak marked with its value.
 */
export function movementOption(result: AxesMapResult): EChartsCoreOption {
  const zones = movementZones(result);
  const series = CHIP_AXIS_COLORS.map((color, k) => {
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
        itemStyle: { color, borderColor: "#000", borderWidth: 0.5 },
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
    lineStyle: { type: "dashed", color: MUTED, width: 2, opacity: 0.6 },
    data: zones.slice(1).map((z) => ({ xAxis: z.start })),
  };
  series[0].markArea = {
    silent: true,
    data: zones.map((z) => [
      {
        xAxis: z.start,
        itemStyle: { color: z.virtual ? VIRTUAL_ZONE : "transparent" },
        label: {
          position: "inside",
          formatter: `{letter|${z.axis}}\n{note|${z.note}}`,
          rich: {
            letter: { fontSize: 56, fontWeight: "bold", color: z.virtual ? VIRTUAL_TEXT : WATERMARK },
            note: {
              fontSize: 11,
              fontWeight: z.virtual ? "normal" : "bold",
              color: z.virtual ? VIRTUAL_TEXT : WATERMARK,
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
    textStyle: { color: TEXT },
    tooltip: { trigger: "axis", valueFormatter: (v: number) => `${v.toFixed(1)} mm/s` },
    legend: { data: series.map((s) => s.name as string), top: 0, left: 56, textStyle: { color: TEXT } },
    graphic: [
      {
        type: "text",
        right: 24,
        top: 0,
        style: { text: `${gravityText(result)}\n${noiseText(result)}`, fill: MUTED, fontSize: 12, align: "right" },
      },
    ],
    grid: { top: 52, left: 64, right: 24, bottom: 40 },
    xAxis: {
      type: "value",
      name: "Time (s)",
      nameLocation: "middle",
      nameGap: 26,
      min: 0,
      max: total,
      splitLine: { lineStyle: { color: GRID } },
    },
    yAxis: { type: "value", name: "Velocity (mm/s)", splitLine: { lineStyle: { color: GRID } } },
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
 * is where chip axis `i` points in machine coordinates. The chip axis behind a virtual machine
 * axis is labelled "(virtual)". Null when there is no rotation.
 */
export function orientationArrows(
  result: AxesMapResult,
): { machine: OrientationArrow[]; chip: OrientationArrow[] } | null {
  const rotation = result.rotation;
  if (rotation === null) return null;
  const virtualChipAxis = result.axes.find((a) => a.state === "virtual")?.chip_axis ?? null;
  const scaled = (v: number[], s: number) => isometric(v.map((c) => c * s));
  const machine = MACHINE_AXES.map((label, i) => {
    const unit = [0, 0, 0];
    unit[i] = 1;
    return { label, color: MUTED, end: scaled(unit, 1.15), labelAt: scaled(unit, 1.3) };
  });
  const chip = CHIP_AXIS_COLORS.map((color, i) => {
    const column = [rotation[0][i], rotation[1][i], rotation[2][i]];
    const name = "xyz"[i];
    const virtual = virtualChipAxis !== null && CHIP_AXIS_INDEX[virtualChipAxis] === i;
    return {
      label: virtual ? `${name} (virtual)` : name,
      color,
      end: scaled(column, 0.9),
      labelAt: scaled(column, 1.05),
    };
  });
  return { machine, chip };
}

const degrees = (value: number) => `${value.toFixed(1)}°`;

/** The detected map, one line per machine axis and the Euler angles, as Shake&Tune's title has them. */
export function orientationHeader(result: AxesMapResult): { map: string; axes: string[]; euler: string | null } {
  const axes = result.axes.map((match, i) => {
    const machine = MACHINE_AXES[i];
    if (match.state === "not_determined" || match.chip_axis === null) return `${machine}: not determined`;
    const detail = match.state === "virtual" ? "(extrapolated)" : `(angle error: ${degrees(match.tilt_deg)})`;
    return `${machine} -> ${chipAxisText(match)} ${detail}`;
  });
  const euler = result.euler_deg
    ? `Euler orientation: X=${degrees(result.euler_deg[0])} Y=${degrees(result.euler_deg[1])} Z=${degrees(result.euler_deg[2])}`
    : null;
  return { map: `Detected axes map: ${result.map ?? "none"}`, axes, euler };
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
  const arrows = orientationArrows(result);
  if (arrows === null) return null;
  const marker = (i: number | "machine") => `${id}-head-${i}`;
  const heads = [
    { key: marker("machine"), color: MUTED },
    ...CHIP_AXIS_COLORS.map((color, i) => ({ key: marker(i), color })),
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
            fill={MUTED}
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
            fontSize={a.label.length > 1 ? 0.12 : 0.18}
            fontWeight="bold"
            textAnchor="middle"
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
  return (
    <EChart
      option={movementOption(result)}
      label={`Movement sequence of ${result.chip}: ${gravityText(result)}, ${noiseText(result)}`}
      height={height}
    />
  );
}

/** Shake&Tune's axes-map plot: the header lines, the movement sequence and the orientation view. */
export function AxesMapPlot({ result }: { result: AxesMapResult }) {
  const header = orientationHeader(result);
  return (
    <div className="space-y-3">
      <div className="space-y-0.5 font-mono text-sm">
        <p className="font-semibold">{header.map}</p>
        {header.axes.map((line) => (
          <p key={line}>{line}</p>
        ))}
        {header.euler && <p>{header.euler}</p>}
      </div>
      <div className="grid gap-4 lg:grid-cols-[5fr_3fr]">
        <div className="relative">
          <MovementChart result={result} />
          <div className="absolute top-6 right-0">
            <ExpandDialog title={`Movement sequence: ${result.chip}`}>
              <MovementChart result={result} height={EXPANDED_CHART_HEIGHT} />
            </ExpandDialog>
          </div>
        </div>
        <div className="flex items-center justify-center">
          <OrientationView result={result} />
        </div>
      </div>
      <ul className="sr-only">
        <li>{gravityText(result)}</li>
        <li>{noiseText(result)}</li>
      </ul>
    </div>
  );
}
