import type { AxisLimits } from "@/lib/generated/AxisLimits";
import type { Direction } from "@/lib/generated/Direction";
import type { Overrides } from "@/lib/generated/Overrides";

/** The seven settings a test may override, in the order the form shows them. */
export const OVERRIDE_FIELDS = [
  { key: "freq_start", label: "Start frequency", unit: "Hz" },
  { key: "freq_end", label: "End frequency", unit: "Hz" },
  { key: "hz_per_sec", label: "Hz per second", unit: "Hz/s" },
  { key: "accel_per_hz", label: "Acceleration per Hz", unit: "mm/s² per Hz" },
  { key: "sweeping_accel", label: "Sweeping acceleration", unit: "mm/s²" },
  { key: "sweeping_period", label: "Sweeping period", unit: "s" },
  { key: "move_speed", label: "Move speed", unit: "mm/s" },
] as const satisfies readonly { key: keyof Overrides; label: string; unit: string }[];

export type OverrideKey = (typeof OVERRIDE_FIELDS)[number]["key"];

/** A duration as minutes and seconds, such as `7 min` or `45 s`. */
export function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  if (total < 60) return `${total} s`;
  const minutes = Math.floor(total / 60);
  const rest = total % 60;
  return rest === 0 ? `${minutes} min` : `${minutes} min ${rest} s`;
}

export type PointPattern = "center" | "five" | "nine";

export const POINT_PATTERNS: { value: PointPattern; label: string }[] = [
  { value: "center", label: "Center" },
  { value: "five", label: "Center + Corners" },
  { value: "nine", label: "3 x 3 Grid" },
];

export const POINT_MARGIN_MM = 20;

const tenth = (value: number) => Math.round(value * 10) / 10;

/**
 * The points of `pattern` over the toolhead's travel at height `z`. The outer points keep
 * [`POINT_MARGIN_MM`] from the limits, less on a bed too small for that. The order keeps the
 * moves between points short.
 */
export function patternPoints(pattern: PointPattern, limits: AxisLimits, z: number): [number, number, number][] {
  const spread = ({ min, max }: { min: number; max: number }): [number, number, number] => {
    const margin = Math.min(POINT_MARGIN_MM, (max - min) / 4);
    return [tenth(min + margin), tenth((min + max) / 2), tenth(max - margin)];
  };
  const [x0, x1, x2] = spread(limits.x);
  const [y0, y1, y2] = spread(limits.y);
  const at = (x: number, y: number): [number, number, number] => [x, y, z];
  switch (pattern) {
    case "center":
      return [at(x1, y1)];
    case "five":
      return [at(x1, y1), at(x0, y0), at(x2, y0), at(x2, y2), at(x0, y2)];
    case "nine":
      return [
        at(x0, y0),
        at(x1, y0),
        at(x2, y0),
        at(x2, y1),
        at(x1, y1),
        at(x0, y1),
        at(x0, y2),
        at(x1, y2),
        at(x2, y2),
      ];
  }
}

/**
 * The progress panel's name for a segment's direction: the belt a diagonal tests, `X Axis` or
 * `Y Axis`, or the vector's own text for any other direction.
 */
export function segmentLabel(axis: Direction): string {
  if (typeof axis === "string") return `${axis} Axis`;
  const [x, y, z] = axis.Vector;
  if (z === 0 && x === 1 && y === -1) return "Belt A (Axis 1,-1)";
  if (z === 0 && x === 1 && y === 1) return "Belt B (Axis 1,1)";
  return axis.Vector.join(",");
}

/** A point as `X 75, Y 75, Z 20 mm`. */
export function formatPosition(point: readonly [number, number, number]): string {
  const [x, y, z] = point.map((v) => String(Math.round(v * 1000) / 1000));
  return `X ${x}, Y ${y}, Z ${z} mm`;
}
