import { LineChart } from "echarts/charts";
import {
  GraphicComponent,
  GridComponent,
  LegendComponent,
  MarkLineComponent,
  MarkPointComponent,
  TitleComponent,
  TooltipComponent,
} from "echarts/components";
import * as echarts from "echarts/core";
import type { EChartsCoreOption } from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import { useEffect, useRef } from "react";

echarts.use([
  LineChart,
  GraphicComponent,
  GridComponent,
  LegendComponent,
  MarkLineComponent,
  MarkPointComponent,
  TitleComponent,
  TooltipComponent,
  CanvasRenderer,
]);

interface Props {
  option: EChartsCoreOption;
  /** What the chart shows, for people who cannot see the canvas. */
  label: string;
  height?: number | string;
  /** Called with the x value (the frequency) of a click on the plot. */
  onClickX?: (x: number) => void;
}

/** A chart that fills its container's width and follows its size. */
export function EChart({ option, label, height = 340, onClickX }: Props) {
  const element = useRef<HTMLDivElement>(null);
  const chart = useRef<echarts.ECharts | null>(null);

  useEffect(() => {
    const instance = echarts.init(element.current);
    chart.current = instance;
    const observer = new ResizeObserver(() => instance.resize());
    observer.observe(element.current!);
    return () => {
      observer.disconnect();
      instance.dispose();
      chart.current = null;
    };
  }, []);

  useEffect(() => {
    chart.current?.setOption(option, true);
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

  return <div ref={element} role="img" aria-label={label} style={{ height }} />;
}
