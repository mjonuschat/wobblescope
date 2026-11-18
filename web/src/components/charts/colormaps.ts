/**
 * Colors that encode data or sit on a heatmap, so they do not change with the theme. These are the
 * inferno ramp of the vibrations heatmaps and the pale text and grid lines Shake&Tune draws over them.
 */
export const INFERNO = ["#000004", "#420a68", "#932667", "#dd513a", "#fca50a", "#fcffa4"];

export const ON_HEATMAP = {
  text: "#fff",
  outline: "#000",
  /** The polar heatmap's rings and spokes. */
  gridStrong: "rgba(176, 186, 198, 0.9)",
  /** The angular energy plot's grid, paler because it lies over a filled area. */
  gridSoft: "rgba(176, 186, 198, 0.45)",
};

export const rampColors = (ramp: readonly (readonly [number, number, number])[]): string[] =>
  ramp.map(([r, g, b]) => `rgb(${r}, ${g}, ${b})`);
