import type { SessionMeta } from "@/lib/generated/SessionMeta";

const ANGLES = 2;

/**
 * How far a vibrations run got, such as "Angle 2 of 2, speed 3 of 12", from the steps the session
 * recorded and the speeds the run was set up with. `null` when the session has no run parameters.
 */
export function vibrationsProgress(meta: SessionMeta): string | null {
  const run = meta.vibrations_run;
  if (!run) return null;
  // The daemon plans the start speed, every step below the end speed, then the end speed.
  const planned = Math.ceil((run.max_speed - run.min_speed) / run.speed_increment - 1e-9) + 1;
  // Once a second segment exists the first is finished, so its recorded count is the true one.
  // That keeps a session recorded before the end speed was added correct.
  const perAngle = meta.segments.length > 1 ? meta.segments[0].steps.length : planned;
  const done = meta.segments.reduce((sum, segment) => sum + segment.steps.length, 0);
  if (done === perAngle) return `Angle 1 of ${ANGLES} finished, no step of angle 2 finished`;
  const angle = Math.min(ANGLES, Math.floor(done / perAngle) + 1);
  const speed = Math.min(perAngle, done - (angle - 1) * perAngle);
  return `Angle ${angle} of ${ANGLES}, speed ${speed} of ${perAngle}`;
}
