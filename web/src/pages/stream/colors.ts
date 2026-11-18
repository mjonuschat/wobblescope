import { type ChartTheme, DEFAULT_CHART_THEME } from "@/ui-kit/chartTheme";

/** The colour of the chip at `index`, the same everywhere on the page. */
export function chipColor(index: number, theme: ChartTheme = DEFAULT_CHART_THEME): string {
  const { series } = theme;
  return series[((index % series.length) + series.length) % series.length];
}
