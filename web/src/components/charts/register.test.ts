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

  it("render every belt chart with paired and unpaired peaks and run the tooltip formatters", () => {
    const result = beltsResult();
    const single = beltSingleOption({ freqs: result.freqs, psd: result.a.psd }, "Belt A");
    for (const option of [beltOverlayOption(result), beltScatterOption(result), single]) {
      expect(render(option)).toContain("<svg");
    }
    type Tooltip = { formatter: (params: unknown) => string };
    const formatter = (option: unknown) => (option as { tooltip: Tooltip }).tooltip.formatter;
    const sample = [{ seriesName: "Belt A", marker: "", value: [787.9369679595126, 0.4] }];
    expect(formatter(beltOverlayOption(result))(sample)).toBe("787.94 Hz<br/>Belt A: 4.000e-1");
    expect(formatter(single)(sample)).toBe("787.94 Hz<br/>Belt A: 4.000e-1");
    expect(formatter(beltScatterOption(result))([{ ...sample[0], seriesName: "Belts" }])).toBe(
      "Belt A: 7.879e+2<br/>Belt B: 4.000e-1",
    );
  });
});
