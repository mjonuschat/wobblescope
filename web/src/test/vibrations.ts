import type { MotorResult } from "@/lib/generated/MotorResult";
import type { VibrationsResult } from "@/lib/generated/VibrationsResult";

export function motorResult(overrides: Partial<MotorResult> = {}): MotorResult {
  return {
    freqs: [10, 20, 30, 40, 50],
    profiles: [
      [0, [1, 2, 4, 2, 1]],
      [90, [1, 3, 5, 3, 1]],
    ],
    combined: [1, 2.5, 4.5, 2.5, 1],
    fr: 30,
    zeta: 0.123,
    peak_index: 2,
    low_freq_warning: false,
    ...overrides,
  };
}

/**
 * A small vibrations result: 9 angles (0 to 360 in steps of 45), 4 speeds, a grid with a zero cell and
 * two peaks, one good speed zone and one good angle zone.
 */
export function vibrationsResult(overrides: Partial<VibrationsResult> = {}): VibrationsResult {
  const angles = [0, 45, 90, 135, 180, 225, 270, 315, 360];
  const speeds = [20, 40, 60, 80];
  return {
    steps: [],
    speeds,
    angles,
    angle_energy: [1, 2, 3, 2, 1, 2, 3, 2, 1],
    grid: angles.map((_, a) => speeds.map((_, s) => (a === 0 && s === 0 ? 0 : (a + 1) * (s + 1) * 0.5))),
    speed_min: [1, 2, 1, 1],
    speed_max: [3, 6, 4, 5],
    speed_variance: [0.5, 1, 0.8, 0.9],
    vibration_metric: [0.2, 1, 0.3, 0.9],
    peaks: [1, 3],
    peak_speeds: [40, 80],
    good_speeds: [{ start: 2, end: 3, energy: 12.5 }],
    good_angles: [{ start: 1, end: 3, energy: 20 }],
    symmetry: 87.65,
    motor: motorResult(),
    measured: [],
    core_version: "0.0.0",
    ...overrides,
  };
}

function linspace(start: number, stop: number, n: number): number[] {
  return Array.from({ length: n }, (_, i) => start + ((stop - start) * i) / (n - 1));
}

/**
 * A vibrations result shaped like the real one: 720 angles from linspace(0, 360, 720), whose step of
 * 360/719 is not a round number, and speeds from linspace(10, 100, 7), which are not round either.
 */
export function linspaceVibrationsResult(overrides: Partial<VibrationsResult> = {}): VibrationsResult {
  const angles = linspace(0, 360, 720);
  const speeds = linspace(10, 100, 7);
  return vibrationsResult({
    angles,
    speeds,
    angle_energy: angles.map((a) => 1 + Math.sin((a * Math.PI) / 180) ** 2),
    grid: angles.map((_, a) => speeds.map((_, s) => (a + 1) * (s + 1) * 0.001)),
    speed_min: speeds.map(() => 1),
    speed_max: speeds.map(() => 2),
    speed_variance: speeds.map(() => 0.5),
    vibration_metric: speeds.map(() => 0.5),
    peaks: [1, 3],
    peak_speeds: [speeds[1], speeds[3]],
    good_speeds: [{ start: 1, end: 3, energy: 1 }],
    ...overrides,
  });
}
