import type { EChartsCoreOption } from "echarts/core";

import type { BeltCurve } from "@/lib/generated/BeltCurve";
import type { BeltSpectrum } from "@/lib/generated/BeltSpectrum";
import type { BeltsResult } from "@/lib/generated/BeltsResult";
import { DEFAULT_CHART_THEME, type ChartTheme } from "@/ui-kit/chartTheme";

import { BADGE_PADDING, base, splitLine, zip } from "./common";
import { formatFrequency, formatPsd } from "./format";

/** Shake&Tune's names for paired peaks, in pair order. */
export const PAIR_LETTERS = "αβγδεζηθικλμνξοπρστυφχψω";

/** The letter of the pair at `order`; past the alphabet it is the number. */
export const pairLetter = (order: number) => PAIR_LETTERS[order] ?? String(order + 1);

const scientific = (value: number) => value.toExponential(1);

/**
 * A peak label drawn as the filled badge of the spectrum charts. `fill` is the belt's colour, or
 * the alert colour for an unpaired peak.
 */
const peakBadge = (label: string, fill: string, theme: ChartTheme) => ({
  formatter: `{badge|${label}}`,
  position: "top",
  color: theme.text,
  fontSize: 13,
  rich: {
    badge: {
      backgroundColor: fill,
      color: theme.onFill(fill),
      fontWeight: "bold",
      align: "center",
      padding: [1, BADGE_PADDING],
      borderRadius: 3,
    },
  },
});

const beltColor = (belt: "a" | "b", theme: ChartTheme) => (belt === "a" ? theme.accent : theme.series[0]);
const beltPrefix = (belt: "a" | "b") => belt.toUpperCase();

/** The label of every unpaired peak is its place in the list of belt A's followed by belt B's. */
function unpairedPoints(result: BeltsResult, place: (belt: BeltCurve, index: number) => number[]) {
  const points: { belt: "a" | "b"; coord: number[]; number: number }[] = [];
  for (const belt of ["a", "b"] as const) {
    for (const index of result[belt].unpaired) {
      points.push({ belt, coord: place(result[belt], index), number: points.length + 1 });
    }
  }
  return points;
}

interface AxisTooltipItem {
  seriesName?: string;
  marker?: string;
  value?: unknown;
}

/** The frequency as the header, then each series with its colour marker and PSD to four significant digits. */
function beltAxisTooltip(params: AxisTooltipItem | AxisTooltipItem[]): string {
  const items = Array.isArray(params) ? params : [params];
  const header = `${formatFrequency(Number((items[0]?.value as number[] | undefined)?.[0]))} Hz`;
  const lines = items.map(
    (item) => `${item.marker ?? ""}${item.seriesName ?? ""}: ${formatPsd(Number((item.value as number[])[1]))}`,
  );
  return [header, ...lines].join("<br/>");
}

/** Only the belt curve is listed: the good zone and the ideal line are drawn aids, not readings. */
function beltScatterTooltip(params: AxisTooltipItem | AxisTooltipItem[]): string {
  const items = Array.isArray(params) ? params : [params];
  const point = items.find((item) => item.seriesName === "Belts")?.value as number[] | undefined;
  if (!point) return "";
  return `Belt A: ${formatPsd(point[0])}<br/>Belt B: ${formatPsd(point[1])}`;
}

/**
 * Box-select and wheel zoom, driven from the page's own buttons (see ChartZoomControls). The
 * toolbox is what registers the box-select brush, and `show: false` would skip it, so it stays
 * shown but is moved off the canvas where it can be neither seen nor clicked. The x axis alone
 * zooms unless `both`.
 */
function zoomControls(both: boolean, theme: ChartTheme): EChartsCoreOption {
  return {
    toolbox: {
      top: -100,
      feature: {
        dataZoom: {
          ...(both ? {} : { yAxisIndex: "none" }),
          // A flat accent box instead of the default gradient.
          brushStyle: { color: theme.alpha(theme.accent, 0.15), borderColor: theme.accent, borderWidth: 1 },
        },
      },
    },
    dataZoom: [{ type: "inside", xAxisIndex: 0, ...(both ? { yAxisIndex: 0 } : {}) }],
  };
}

/**
 * An axis name centred along its axis rather than at the axis end, where it would run into the
 * legend and zoom buttons. In the 380 px chart the grid spans y 40 to 324: the legend and buttons keep
 * the 40 px above it. A name 52 px left of the axis (scientific tick labels are about 40 px wide)
 * spans roughly 45 to 59 px, inside the 72 px left of the grid. "Belt A" sits 30 px under the
 * axis inside the 56 px below the grid.
 */
const axisName = (name: string, gap: number) => ({ name, nameLocation: "middle" as const, nameGap: gap });

const PSD_AXIS_NAME = axisName("Power spectral density", 52);
const BELT_GRID = { top: 40, left: 72, right: 24, bottom: 40 };

const highest = (result: BeltsResult) => Math.max(...result.a.psd, ...result.b.psd);

/**
 * Both belts' PSDs. Paired peaks carry a badge in the belt's colour, `A α` or `B α`, and are joined by a
 * dotted line; unpaired peaks carry a red badge with the belt and a number. The y axis ends at 1.1 times the
 * highest value, as Shake&Tune.
 */
export function beltOverlayOption(result: BeltsResult, theme: ChartTheme = DEFAULT_CHART_THEME): EChartsCoreOption {
  const marks = (belt: "a" | "b") => {
    return [
      ...result.pairs.map((pair, order) => ({
        coord: [pair[belt].freq, pair[belt].level],
        label: peakBadge(`${beltPrefix(belt)} ${pairLetter(order)}`, beltColor(belt, theme), theme),
      })),
      ...unpairedPoints(result, (curve, index) => [result.freqs[index], curve.psd[index]])
        .filter((point) => point.belt === belt)
        .map((point) => ({
          coord: point.coord,
          label: peakBadge(`${beltPrefix(belt)} ${point.number}`, theme.danger, theme),
        })),
    ];
  };
  const curve = (name: string, belt: "a" | "b", color: string) => ({
    name,
    type: "line",
    showSymbol: false,
    symbol: "none",
    color,
    lineStyle: { color },
    data: zip(result.freqs, result[belt].psd),
    markPoint: {
      silent: true,
      symbol: "circle",
      symbolSize: 6,
      itemStyle: { color, borderWidth: 0 },
      data: marks(belt),
    },
    ...(belt === "a"
      ? {
          markLine: {
            silent: true,
            symbol: "none",
            lineStyle: { color: theme.muted, type: "dotted", width: 1 },
            label: { show: false },
            data: result.pairs.map((pair) => [
              { coord: [pair.a.freq, pair.a.level] },
              { coord: [pair.b.freq, pair.b.level] },
            ]),
          },
        }
      : {}),
  });
  return {
    ...base(null, result.max_freq, theme),
    animation: false,
    tooltip: { trigger: "axis", formatter: beltAxisTooltip },
    grid: BELT_GRID,
    legend: { type: "scroll", data: ["Belt A", "Belt B"], top: 0, left: 56, textStyle: { color: theme.text } },
    ...zoomControls(false, theme),
    yAxis: {
      type: "value",
      ...PSD_AXIS_NAME,
      splitLine: splitLine(theme),
      min: 0,
      max: highest(result) * 1.1,
      axisLabel: { formatter: scientific },
    },
    series: [curve("Belt A", "a", theme.accent), curve("Belt B", "b", theme.series[0])],
  };
}

/** One belt's PSD alone, for a session where the other belt has no usable recording. */
export function beltSingleOption(
  spectrum: BeltSpectrum,
  label: string,
  theme: ChartTheme = DEFAULT_CHART_THEME,
): EChartsCoreOption {
  const top = Math.max(...spectrum.psd);
  return {
    ...base(null, spectrum.freqs.at(-1) ?? 0, theme),
    animation: false,
    tooltip: { trigger: "axis", formatter: beltAxisTooltip },
    grid: BELT_GRID,
    legend: { type: "scroll", data: [label], top: 0, left: 56, textStyle: { color: theme.text } },
    ...zoomControls(false, theme),
    yAxis: {
      type: "value",
      ...PSD_AXIS_NAME,
      splitLine: splitLine(theme),
      min: 0,
      max: top * 1.1,
      axisLabel: { formatter: scientific },
    },
    series: [
      {
        name: label,
        type: "line",
        showSymbol: false,
        symbol: "none",
        color: beltColor(label.startsWith("A") ? "a" : "b", theme),
        data: zip(spectrum.freqs, spectrum.psd),
      },
    ],
  };
}

/** Where the good zone ends above `x`, as Shake&Tune draws it. */
export const goodZoneBoundary = (x: number, max: number) => x + 0.35 * max * Math.exp(-x / (0.6 * max));

/** The y at which the boundary reaches `x`: the mirror of the zone's upper edge, never below 0. */
function goodZoneLowerEdge(x: number, max: number): number {
  let low = 0;
  let high = x;
  for (let i = 0; i < 40; i += 1) {
    const mid = (low + high) / 2;
    if (goodZoneBoundary(mid, max) < x) low = mid;
    else high = mid;
  }
  return goodZoneBoundary(0, max) > x ? 0 : low;
}

const ZONE_STEPS = 100;

/**
 * Belt A's PSD against belt B's, with the ideal line, the good zone and the paired peaks. Where
 * the two peaks of a pair are within 1 Hz they are one point labelled with the letter alone; otherwise each belt has its own `A α` and `B α` point.
 */
export function beltScatterOption(result: BeltsResult, theme: ChartTheme = DEFAULT_CHART_THEME): EChartsCoreOption {
  const max = highest(result);
  const end = max * 1.1;
  const xs = Array.from({ length: ZONE_STEPS + 1 }, (_, i) => (end * i) / ZONE_STEPS);
  const lower = xs.map((x) => goodZoneLowerEdge(x, max));
  const { a, b } = result;
  const curve = a.psd.map((x, i) => [x, b.psd[i]]);
  const dot = (coord: number[], label: string, color: string, fill = color) => ({
    coord,
    itemStyle: { color, borderWidth: 0 },
    label: peakBadge(label, fill, theme),
  });
  const points = [
    ...result.pairs.flatMap((pair, order) => {
      const letter = pairLetter(order);
      if (Math.abs(pair.a.freq - pair.b.freq) < 1) {
        return [dot([a.psd[pair.a.index], b.psd[pair.b.index]], letter, theme.text, theme.muted)];
      }
      return [
        dot([a.psd[pair.b.index], b.psd[pair.b.index]], `B ${letter}`, theme.series[0]),
        dot([a.psd[pair.a.index], b.psd[pair.a.index]], `A ${letter}`, theme.accent),
      ];
    }),
    ...unpairedPoints(result, (_, index) => [a.psd[index], b.psd[index]]).map((point) =>
      dot(point.coord, `${beltPrefix(point.belt)} ${point.number}`, beltColor(point.belt, theme), theme.danger),
    ),
  ];
  return {
    ...base(null, end, theme),
    animation: false,
    tooltip: { trigger: "axis", formatter: beltScatterTooltip },
    grid: { top: 40, left: 72, right: 24, bottom: 56 },
    legend: { type: "scroll", data: ["Ideal line", "Good zone"], top: 0, left: 56, textStyle: { color: theme.text } },
    ...zoomControls(true, theme),
    xAxis: {
      ...axisName("Belt A", 30),
      type: "value",
      min: 0,
      max: end,
      splitLine: splitLine(theme),
      axisLabel: { formatter: scientific },
    },
    yAxis: {
      ...axisName("Belt B", 52),
      type: "value",
      min: 0,
      max: end,
      splitLine: splitLine(theme),
      axisLabel: { formatter: scientific },
    },
    series: [
      {
        name: "Good zone floor",
        type: "line",
        stack: "zone",
        showSymbol: false,
        symbol: "none",
        silent: true,
        lineStyle: { opacity: 0 },
        tooltip: { show: false },
        data: xs.map((x, i) => [x, lower[i]]),
      },
      {
        name: "Good zone",
        type: "line",
        stack: "zone",
        showSymbol: false,
        symbol: "none",
        silent: true,
        lineStyle: { opacity: 0 },
        tooltip: { show: false },
        areaStyle: { color: theme.alpha(theme.success, 0.15) },
        itemStyle: { color: theme.alpha(theme.success, 0.15) },
        data: xs.map((x, i) => [x, goodZoneBoundary(x, max) - lower[i]]),
      },
      {
        name: "Ideal line",
        type: "line",
        showSymbol: false,
        symbol: "none",
        silent: true,
        tooltip: { show: false },
        color: theme.danger,
        lineStyle: { color: theme.danger, type: "dashed", width: 2 },
        data: [
          [0, 0],
          [end, end],
        ],
      },
      {
        // ECharts areas only fill vertically, so the horizontal fill to the y axis is a polygon.
        name: "Belt curve shading",
        type: "custom",
        silent: true,
        tooltip: { show: false },
        z: 0,
        data: [0],
        renderItem: (_: unknown, api: { coord: (point: number[]) => number[] }) => ({
          type: "polygon",
          shape: {
            points: [
              ...curve.map(([x, y]) => api.coord([x, y])),
              api.coord([0, curve[curve.length - 1][1]]),
              api.coord([0, curve[0][1]]),
            ],
          },
          style: { fill: theme.alpha(theme.danger, 0.1) },
        }),
      },
      {
        name: "Belts",
        type: "line",
        showSymbol: false,
        symbol: "none",
        color: theme.muted,
        lineStyle: { color: theme.muted, width: 1 },
        data: curve,
        markPoint: { silent: true, symbol: "circle", symbolSize: 8, data: points },
      },
    ],
  };
}
