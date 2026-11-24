import type { Axis } from "@/lib/generated/Axis";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

export function sessionAxes(meta: SessionMeta): Axis[] {
  return [...new Set(meta.segments.map((s) => s.axis).filter((a): a is Axis => typeof a === "string"))];
}
