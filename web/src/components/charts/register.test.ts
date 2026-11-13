import * as echarts from "echarts/core";
import { SVGRenderer } from "echarts/renderers";
import { describe, expect, it } from "vitest";

import { beltsResult } from "@/test/fixtures";

import { beltOverlayOption, beltScatterOption, beltSingleOption } from "./options";
import { CHART_PARTS } from "./register";

// The component tests mock EChart, so only a real setOption shows a series type that is not registered.
describe("the registered chart parts", () => {
  echarts.use([...CHART_PARTS, SVGRenderer]);
  const render = (option: Parameters<echarts.ECharts["setOption"]>[0]) => {
    const chart = echarts.init(null, null, { renderer: "svg", ssr: true, width: 600, height: 400 });
    chart.setOption(option);
    const svg = chart.renderToSVGString();
    chart.dispose();
    return svg;
  };

  it("draw the belt overlay", () => {
    expect(render(beltOverlayOption(beltsResult()))).toContain("<svg");
  });

  it("zoom every belt chart from dispatched actions", () => {
    const result = beltsResult();
    for (const option of [
      beltOverlayOption(result),
      beltScatterOption(result),
      beltSingleOption({ freqs: result.freqs, psd: result.a.psd }, "Belt A"),
    ]) {
      const chart = echarts.init(null, null, { renderer: "svg", ssr: true, width: 600, height: 400 });
      chart.setOption(option);
      type Zoom = { start: number; end: number }[];
      const ranges = () => (chart.getOption() as { dataZoom: Zoom }).dataZoom.map(({ start, end }) => [start, end]);
      const full = ranges();
      chart.dispatchAction({ type: "takeGlobalCursor", key: "dataZoomSelect", dataZoomSelectActive: true });
      chart.dispatchAction({ type: "dataZoom", dataZoomIndex: 0, start: 20, end: 60 });
      expect(ranges()[0]).toEqual([20, 60]);
      chart.dispatchAction({ type: "restore" });
      expect(ranges()).toEqual(full);
      chart.dispose();
    }
  });

  it("draw the belt scatter with its good zone", () => {
    expect(render(beltScatterOption(beltsResult()))).toContain("<svg");
  });

  it("draw a single belt", () => {
    const result = beltsResult();
    expect(render(beltSingleOption({ freqs: result.freqs, psd: result.a.psd }, "Belt A"))).toContain("<svg");
  });
});
