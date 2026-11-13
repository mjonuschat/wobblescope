import { CustomChart, HeatmapChart, LineChart } from "echarts/charts";
import {
  BrushComponent,
  DataZoomComponent,
  GraphicComponent,
  GridComponent,
  LegendComponent,
  MarkAreaComponent,
  MarkLineComponent,
  MarkPointComponent,
  TitleComponent,
  ToolboxComponent,
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
  BrushComponent,
  DataZoomComponent,
  GraphicComponent,
  GridComponent,
  LegendComponent,
  MarkAreaComponent,
  MarkLineComponent,
  MarkPointComponent,
  TitleComponent,
  ToolboxComponent,
  TooltipComponent,
  VisualMapContinuousComponent,
];

echarts.use([...CHART_PARTS, CanvasRenderer]);
