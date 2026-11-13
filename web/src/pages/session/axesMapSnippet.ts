/**
 * The Klipper config lines that set a chip's axes map. `chip.name` is the full section name,
 * such as `adxl345 nh36`. The option name follows the chip type, as `moonraker/chips.rs` reads it.
 */
export function axesMapSnippet(chip: { name: string; chip_type: string | null }, map: string): string {
  const option = chip.chip_type?.toLowerCase() === "beacon" ? "accel_axes_map" : "axes_map";
  return `[${chip.name}]\n${option}: ${map}\n`;
}
