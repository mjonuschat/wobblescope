import { CustomChart, HeatmapChart, LineChart } from "echarts/charts";
import {
  GraphicComponent,
  GridComponent,
  LegendComponent,
  MarkAreaComponent,
  MarkLineComponent,
  MarkPointComponent,
  TitleComponent,
  TooltipComponent,
  VisualMapContinuousComponent,
} from "echarts/components";
import * as echarts from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";

/** Every chart and component the app's options use; ECharts throws on a series type left out. */
export const CHART_PARTS = [
  CustomChart,
  HeatmapChart,
  LineChart,
  GraphicComponent,
  GridComponent,
  LegendComponent,
  MarkAreaComponent,
  MarkLineComponent,
  MarkPointComponent,
  TitleComponent,
  TooltipComponent,
  VisualMapContinuousComponent,
];

echarts.use([...CHART_PARTS, CanvasRenderer]);
