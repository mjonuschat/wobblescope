import { Panel } from "@/components/Panel";
import { pointLabel } from "@/components/charts/options";
import type { Direction } from "@/lib/generated/Direction";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

/** `X`, `Y`, or the vector's components, without a trailing zero z (`1,-1`). */
export function directionLabel(direction: Direction): string {
  if (typeof direction === "string") return direction;
  const [x, y, z] = direction.Vector;
  return (z === 0 ? [x, y] : [x, y, z]).join(",");
}

/** The session is valid but this version cannot analyse its directions, so it lists what it holds. */
export function UnsupportedDirections({ meta }: { meta: SessionMeta }) {
  return (
    <Panel title="Recordings">
      <p role="status" className="text-sm">
        This version has no analysis for these directions.
      </p>
      <ul aria-label="Segments" className="space-y-1 text-sm">
        {meta.segments.map((segment, index) => (
          // A session's segments do not change, and no field of one is unique.
          // biome-ignore lint/suspicious/noArrayIndexKey: see above
          <li key={index}>
            Direction {directionLabel(segment.axis)} at {pointLabel(segment.point, meta.probe_points)}
          </li>
        ))}
      </ul>
    </Panel>
  );
}
