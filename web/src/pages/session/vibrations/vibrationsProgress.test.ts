import { describe, expect, it } from "vitest";

import type { SessionMeta } from "@/lib/generated/SessionMeta";
import { twoAxisMeta } from "@/test/fixtures";

import { vibrationsProgress } from "./vibrationsProgress";

function metaWith(...done: number[]): SessionMeta {
  const meta = twoAxisMeta();
  meta.vibrations_run = {
    size: 100,
    z_height: 20,
    max_speed: 100,
    speed_increment: 20,
    accel: 3000,
    travel_speed: 120,
    min_speed: 20,
    kinematics: "corexy",
  };
  const step = (i: number) => ({ speed: 20 + 20 * i, window: { start: 0, end: 1 } });
  meta.segments = done.map((n) => ({
    axis: { Vector: [1, 0, 0] },
    point: 0,
    window: null,
    files: [],
    steps: Array.from({ length: n }, (_, i) => step(i)),
  })) as SessionMeta["segments"];
  return meta;
}

describe("vibrationsProgress", () => {
  it("counts_the_speeds_of_the_first_angle", () => {
    expect(vibrationsProgress(metaWith(3))).toBe("Angle 1 of 2, speed 3 of 5");
  });

  it("counts_the_end_speed_when_the_step_does_not_divide_the_range", () => {
    const meta = metaWith(3);
    meta.vibrations_run!.speed_increment = 30;
    expect(vibrationsProgress(meta)).toBe("Angle 1 of 2, speed 3 of 4");
  });

  it("takes_the_per_angle_count_from_a_finished_first_segment_of_an_older_session", () => {
    const meta = metaWith(4, 1);
    meta.vibrations_run!.speed_increment = 30;
    expect(vibrationsProgress(meta)).toBe("Angle 2 of 2, speed 1 of 4");
  });

  it("says_the_second_angle_is_not_started_when_the_first_is_exactly_finished", () => {
    expect(vibrationsProgress(metaWith(5))).toBe("Angle 1 of 2 finished, no step of angle 2 finished");
    expect(vibrationsProgress(metaWith())).toBe("Angle 1 of 2, speed 0 of 5");
  });

  it("counts_the_x_and_y_segments_of_a_cartesian_printer", () => {
    const meta = metaWith(5, 2);
    meta.vibrations_run!.kinematics = "cartesian";
    meta.segments[0].axis = "X";
    meta.segments[1].axis = "Y";
    expect(vibrationsProgress(meta)).toBe("Angle 2 of 2, speed 2 of 5");
  });

  it("stopped_inside_the_second_angle", () => {
    expect(vibrationsProgress(metaWith(5, 2))).toBe("Angle 2 of 2, speed 2 of 5");
  });

  it("has_no_progress_without_the_run_parameters", () => {
    const meta = metaWith(2);
    meta.vibrations_run = null;
    expect(vibrationsProgress(meta)).toBeNull();
  });
});

describe("vibrationsProgress at more directions", () => {
  it("recovered_progress_counts_directions_from_the_run_record", () => {
    const meta = metaWith(12, 12, 12, 12, 12, 12, 3);
    meta.vibrations_run!.max_speed = 240;
    meta.vibrations_run!.angular_step = 15;
    expect(vibrationsProgress(meta)).toBe("Angle 7 of 12, speed 3 of 12");
  });

  it("names_the_last_finished_direction_of_a_longer_run", () => {
    const meta = metaWith(5, 5);
    meta.vibrations_run!.angular_step = 45;
    expect(vibrationsProgress(meta)).toBe("Angle 2 of 4 finished, no step of angle 3 finished");
  });

  it("recovered_progress_for_a_motor_only_run_is_unchanged", () => {
    const meta = metaWith(5, 2);
    meta.vibrations_run!.angular_step = null;
    expect(vibrationsProgress(meta)).toBe("Angle 2 of 2, speed 2 of 5");
  });
});
