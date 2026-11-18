import { useMemo, useSyncExternalStore } from "react";

import { parseOklch, type Rgba, readableOn, toCss, withAlpha } from "@/ui-kit/color";

/**
 * The colors a chart needs, read from the theme tokens. ECharts draws to a canvas and cannot read
 * CSS variables, so each theme change reads them again (see `useChartTheme`).
 */
export interface ChartTheme {
  text: string;
  muted: string;
  grid: string;
  /** The panel surface, for a label that must hide the lines under it. */
  surface: string;
  /** The lead series, the one the chart is about. */
  accent: string;
  /** Neutral steps from strong to faint, for the series that are not the lead. */
  neutral: readonly [string, string, string, string];
  /** Categorical colors for series that must be told apart, at hues the accent and status colors avoid. */
  series: readonly string[];
  success: string;
  warning: string;
  danger: string;
  onFill: (fill: string) => string;
  alpha: (color: string, alpha: number) => string;
}

const TOKENS = {
  text: "--foreground",
  muted: "--muted-foreground",
  grid: "--border",
  surface: "--card",
  accent: "--chart-1",
  success: "--success",
  warning: "--warning",
  danger: "--destructive",
} as const;
const NEUTRALS = ["--chart-2", "--chart-3", "--chart-4", "--chart-5"] as const;
const SERIES = ["--series-1", "--series-2", "--series-3", "--series-4", "--series-5"] as const;

/** The dark theme's token values, for a page that has no stylesheet (tests). A test keeps them in step with tokens.css. */
export const FALLBACK_TOKENS: Record<string, string> = {
  "--foreground": "oklch(0.96 0.003 260)",
  "--muted-foreground": "oklch(0.7 0.005 260)",
  "--border": "oklch(1 0 0 / 10%)",
  "--card": "oklch(0.205 0.005 260)",
  "--chart-1": "oklch(0.682 0.211 37.7)",
  "--chart-2": "oklch(0.78 0.005 260)",
  "--chart-3": "oklch(0.62 0.005 260)",
  "--chart-4": "oklch(0.5 0.005 260)",
  "--chart-5": "oklch(0.38 0.005 260)",
  "--series-1": "oklch(0.74 0.13 295)",
  "--series-2": "oklch(0.8 0.11 205)",
  "--series-3": "oklch(0.74 0.16 350)",
  "--series-4": "oklch(0.82 0.15 125)",
  "--series-5": "oklch(0.72 0.14 265)",
  "--success": "oklch(0.74 0.12 175)",
  "--warning": "oklch(0.82 0.15 85)",
  "--destructive": "oklch(0.68 0.19 22)",
};

/** Builds a theme from a token reader. A token the reader does not know falls back to the dark value. */
export function chartThemeFrom(read: (token: string) => string): ChartTheme {
  const color = (token: string) => {
    return toCss(parseOklch(read(token)) ?? (parseOklch(FALLBACK_TOKENS[token]) as Rgba));
  };
  const neutral = NEUTRALS.map(color) as unknown as ChartTheme["neutral"];
  return {
    text: color(TOKENS.text),
    muted: color(TOKENS.muted),
    grid: color(TOKENS.grid),
    surface: color(TOKENS.surface),
    accent: color(TOKENS.accent),
    neutral,
    series: SERIES.map(color),
    success: color(TOKENS.success),
    warning: color(TOKENS.warning),
    danger: color(TOKENS.danger),
    onFill: readableOn,
    alpha: withAlpha,
  };
}

/** The theme of the dark tokens, used when no theme is passed. */
export const DEFAULT_CHART_THEME: ChartTheme = chartThemeFrom(() => "");

function readFromDocument(): ChartTheme {
  const style = getComputedStyle(document.documentElement);
  return chartThemeFrom((token) => style.getPropertyValue(token));
}

function subscribe(onChange: () => void): () => void {
  const observer = new MutationObserver(onChange);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
  return () => observer.disconnect();
}

const isDark = () => document.documentElement.classList.contains("dark");

/**
 * The chart theme of the page as it is now. It follows the `dark` class of the root element, so a
 * chart built from it redraws when the theme changes.
 */
export function useChartTheme(): ChartTheme {
  const dark = useSyncExternalStore(subscribe, isDark, () => true);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `dark` is the trigger to read the tokens again
  return useMemo(readFromDocument, [dark]);
}
