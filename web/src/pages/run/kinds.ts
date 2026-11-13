import type { TestKind } from "@/lib/generated/TestKind";

/** The kinematics whose belts a belt test can tell apart. */
const BELT_KINEMATICS = ["corexy", "limited_corexy"];

export const offersBelts = (kinematics: string | null | undefined) => BELT_KINEMATICS.includes(kinematics ?? "");

export const KIND_TITLES: Record<TestKind, string> = {
  axes: "Input Shaper",
  belts: "Belts",
  axes_map: "Axes map calibration",
};

/** The run page that shows a test of each kind. */
export const KIND_PAGES = {
  axes: "/run/shaper",
  belts: "/run/belts",
  axes_map: "/run/axes-map",
} as const satisfies Record<TestKind, string>;
