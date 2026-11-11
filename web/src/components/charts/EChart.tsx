import { LineChart } from "echarts/charts";
import {
  GraphicComponent,
  GridComponent,
  LegendComponent,
  MarkLineComponent,
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
  TitleComponent,
  TooltipComponent,
  CanvasRenderer,
]);

interface Props {
  option: EChartsCoreOption;
  /** What the chart shows, for people who cannot see the canvas. */
  label: string;
  height?: number | string;
}

/** A chart that fills its container's width and follows its size. */
export function EChart({ option, label, height = 340 }: Props) {
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

  return <div ref={element} role="img" aria-label={label} style={{ height }} />;
}
