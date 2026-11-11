import { render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Spectrum } from "@/lib/generated/Spectrum";

import { SpectrumChart } from "./SpectrumChart";

interface ChartProps {
  option: { series: { markLine?: { data: { xAxis: number }[] } }[] };
  onClickX?: (x: number) => void;
}
let chart: ChartProps;
vi.mock("@/components/charts/EChart", () => ({
  EChart: (props: ChartProps) => {
    chart = props;
    return <div data-testid="chart" />;
  },
}));

const spectrum: Spectrum = {
  freq_bins: [0, 10, 20],
  psd_x: [1, 2, 3],
  psd_y: [1, 2, 3],
  psd_z: [1, 2, 3],
  psd_sum: [3, 6, 9],
};
const chips = [{ chip: "a", spectrum, peaks: [{ freq: 52.4, level: 6 }], peakHold: [3, 6, 9], axes: ["sum" as const] }];

beforeEach(() => {
  chart = undefined as never;
});

describe("SpectrumChart", () => {
  it("marks the frequency of interest", () => {
    render(<SpectrumChart chips={chips} maxHz={200} frequencyOfInterest={52} onPick={() => {}} />);
    expect(chart.option.series[0].markLine?.data[0].xAxis).toBe(52);
  });

  it("picks a labelled peak near a click, else the clicked frequency to 0.1 Hz", () => {
    const onPick = vi.fn();
    render(<SpectrumChart chips={chips} maxHz={200} frequencyOfInterest={null} onPick={onPick} />);
    chart.onClickX?.(51.2);
    expect(onPick).toHaveBeenLastCalledWith(52.4);
    chart.onClickX?.(70.04);
    expect(onPick).toHaveBeenLastCalledWith(70);
  });
});
