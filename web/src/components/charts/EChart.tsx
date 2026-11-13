import * as echarts from "echarts/core";
import type { EChartsCoreOption } from "echarts/core";
import { useEffect, useRef } from "react";

import { ChartZoomControls } from "./ChartZoomControls";
import "./register";

interface Props {
  option: EChartsCoreOption;
  /** What the chart shows, for people who cannot see the canvas. */
  label: string;
  height?: number | string;
  /** Called with the x value (the frequency) of a click on the plot. */
  onClickX?: (x: number) => void;
  /** Adds zoom buttons at the top right. The option must set up the zoom (see the belt options). */
  zoom?: boolean;
}

/** A chart that fills its container's width and follows its size. */
export function EChart({ option, label, height = 340, onClickX, zoom }: Props) {
  const element = useRef<HTMLDivElement>(null);
  const chart = useRef<echarts.ECharts | null>(null);
  const pointer = useRef<{ x: number; y: number } | null>(null);

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
    const observer = new ResizeObserver(() => instance.resize());
    observer.observe(element.current!);
    return () => {
      zr.off("mousemove", move);
      zr.off("globalout", leave);
      observer.disconnect();
      instance.dispose();
      chart.current = null;
    };
  }, []);

  useEffect(() => {
    const instance = chart.current;
    if (!instance) return;
    instance.setOption(option, true);
    // A full replace closes the tooltip, which on a chart that updates 10 times a second makes
    // it flicker under a resting pointer. Opening it again here happens before the next paint.
    const at = pointer.current;
    if (at) instance.dispatchAction({ type: "showTip", x: at.x, y: at.y });
  }, [option]);

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

  const canvas = <div ref={element} role="img" aria-label={label} style={{ height }} />;
  if (!zoom) return canvas;
  return (
    <div className="relative">
      {canvas}
      <ChartZoomControls chart={chart} />
    </div>
  );
}
