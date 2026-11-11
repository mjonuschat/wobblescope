/** One of the three accelerometer axes, or their summed spectrum. */
export type Axis3 = "x" | "y" | "z" | "sum";

/** How the page names each spectrum line. */
export const AXIS_LABEL: Record<Axis3, string> = { x: "X", y: "Y", z: "Z", sum: "Total" };
