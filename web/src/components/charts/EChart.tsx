import * as echarts from "echarts/core";
import type { EChartsCoreOption } from "echarts/core";
import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";

import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";

import { ChartZoomControls } from "./ChartZoomControls";
import "./register";

interface LegendRow {
  top: number;
  names: string[];
}

function legendRows(option: EChartsCoreOption): LegendRow[] {
  const legends = [(option as { legend?: unknown }).legend].flat().filter(Boolean) as {
    top?: number;
    data?: string[];
  }[];
  return legends.map((l) => ({ top: l.top ?? 0, names: l.data ?? [] }));
}

type LegendOption = { selected?: Record<string, boolean> };

/**
 * `option` with the legend choices the chart currently holds, so a redraw for another reason (a new
 * scale, the theme) does not bring back the curves the user hid or hide the ones they showed.
 */
function keepLegendChoices(option: EChartsCoreOption, current: { legend?: LegendOption[] } | undefined) {
  const chosen = current?.legend;
  const legend = (option as { legend?: LegendOption | LegendOption[] }).legend;
  if (!chosen || chosen.length === 0 || !legend) return option;
  // Only the curves the new legend lists: a choice about one that is gone must not come back later.
  const merge = (l: LegendOption & { data?: unknown[] }, i: number) => {
    const listed = new Set(namesOf(l));
    const kept = Object.entries(chosen[i]?.selected ?? {}).filter(([name]) => listed.has(name));
    return { ...l, selected: { ...l.selected, ...Object.fromEntries(kept) } };
  };
  return { ...option, legend: Array.isArray(legend) ? legend.map(merge) : merge(legend, 0) };
}

interface Props {
  option: EChartsCoreOption;
  /** What the chart shows, for people who cannot see the canvas. */
  label: string;
  height?: number | string;
  /** The chart keeps at least this width, in pixels, and scrolls sideways in a narrower container. */
  minWidth?: number;
  /** Called with the x value (the frequency) of a click on the plot. */
  onClickX?: (x: number) => void;
  /** Adds zoom buttons at the top right. The option must set up the zoom (see the belt options). */
  zoom?: boolean;
  /**
   * One label per legend of the option, in order: each gets a switch beside it that shows or hides
   * every curve of that legend. The option leaves room for it (see `LEGEND_LEFT`).
   */
  legendSwitches?: string[];
}

/** A chart that fills its container's width and follows its size. */
export function EChart({ option, label, height = 340, minWidth, onClickX, zoom, legendSwitches }: Props) {
  const element = useRef<HTMLDivElement>(null);
  const chart = useRef<echarts.ECharts | null>(null);
  const pointer = useRef<{ x: number; y: number } | null>(null);
  const switchId = useId();
  const switched = legendSwitches !== undefined;
  const rows = useMemo(() => (switched ? legendRows(option) : []), [option, switched]);
  const [allShown, setAllShown] = useState<boolean[]>([]);
  const refreshLegend = useCallback(() => {
    const instance = chart.current;
    if (!instance || rows.length === 0) return;
    const legends = [instance.getOption().legend].flat() as ({ selected?: Record<string, boolean> } | undefined)[];
    setAllShown(rows.map((row, i) => row.names.every((name) => legends[i]?.selected?.[name] !== false)));
  }, [rows]);
  const refreshRef = useRef(refreshLegend);
  refreshRef.current = refreshLegend;

  useEffect(() => {
    const instance = echarts.init(element.current);
    chart.current = instance;
    const zr = instance.getZr();
    const move = (event: { offsetX: number; offsetY: number }) => {
      pointer.current = { x: event.offsetX, y: event.offsetY };
    };
    const leave = () => {
      pointer.current = null;
    };
    zr.on("mousemove", move);
    zr.on("globalout", leave);
    const changed = () => refreshRef.current();
    const legendEvents = ["legendselectchanged", "legendselectall", "legendinverseselect"];
    for (const name of legendEvents) instance.on(name, changed);
    const observer = new ResizeObserver(() => instance.resize());
    observer.observe(element.current!);
    return () => {
      zr.off("mousemove", move);
      zr.off("globalout", leave);
      for (const name of legendEvents) instance.off(name, changed);
      observer.disconnect();
      instance.dispose();
      chart.current = null;
    };
  }, []);

  useEffect(() => {
    const instance = chart.current;
    if (!instance) return;
    instance.setOption(
      keepLegendChoices(option, instance.getOption() as { legend?: LegendOption[] } | undefined),
      true,
    );
    refreshLegend();
    // A full replace closes the tooltip, which on a chart that updates 10 times a second makes
    // it flicker under a resting pointer. Opening it again here happens before the next paint.
    const at = pointer.current;
    if (at) instance.dispatchAction({ type: "showTip", x: at.x, y: at.y });
  }, [option, refreshLegend]);

  const clickRef = useRef(onClickX);
  clickRef.current = onClickX;
  const clickable = onClickX !== undefined;
  useEffect(() => {
    const instance = chart.current;
    if (!instance || !clickable) return;
    const zr = instance.getZr();
    const handler = (event: { offsetX: number; offsetY: number }) => {
      const at = instance.convertFromPixel({ gridIndex: 0 }, [event.offsetX, event.offsetY]);
      if (Array.isArray(at) && Number.isFinite(at[0])) clickRef.current?.(at[0]);
    };
    zr.on("click", handler);
    return () => zr.off("click", handler);
  }, [clickable]);

  function showRow(index: number, show: boolean) {
    const instance = chart.current;
    if (!instance) return;
    if (show) instance.dispatchAction({ type: "legendAllSelect", legendIndex: index });
    else
      for (const name of rows[index].names)
        instance.dispatchAction({ type: "legendUnSelect", legendIndex: index, name });
    refreshLegend();
  }

  const canvas = (
    <div className="relative" style={{ minWidth }}>
      <div ref={element} role="img" aria-label={label} style={{ height, minWidth }} />
      {rows.slice(0, legendSwitches?.length ?? 0).map((row, i) => (
        <div
          key={legendSwitches?.[i]}
          className="absolute left-0 flex h-7 w-20 items-center justify-between"
          style={{ top: row.top }}
        >
          <Label htmlFor={`${switchId}-${i}`} className="text-xs">
            {legendSwitches?.[i]}
          </Label>
          <Switch
            id={`${switchId}-${i}`}
            size="sm"
            checked={allShown[i] ?? false}
            onCheckedChange={(show) => showRow(i, show)}
            aria-label={`Show all ${legendSwitches?.[i]?.toLowerCase()}`}
          />
        </div>
      ))}
    </div>
  );
  const body = minWidth ? <div className="overflow-x-auto">{canvas}</div> : canvas;
  if (!zoom) return body;
  return (
    <div className="relative">
      {body}
      <ChartZoomControls chart={chart} />
    </div>
  );
}
