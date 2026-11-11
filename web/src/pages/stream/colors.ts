const CHIP_COLORS = ['#4fc3f7', '#ffb74d', '#81c784', '#ba68c8']

/** The colour of the chip at `index`, the same everywhere on the page. */
export function chipColor(index: number): string {
  return CHIP_COLORS[((index % CHIP_COLORS.length) + CHIP_COLORS.length) % CHIP_COLORS.length]
}
