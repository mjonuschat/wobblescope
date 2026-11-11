import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Spectrum } from "@/lib/generated/Spectrum";

import { EnergyStrip } from "./EnergyStrip";

interface ChartProps {
  option: { series: { name: string; data: number[][] }[]; yAxis: { name: string } };
}
let chart: ChartProps;
vi.mock("@/components/charts/EChart", () => ({
  EChart: (props: ChartProps) => {
    chart = props;
    return <div data-testid="chart" />;
  },
}));

function flat(level: number): Spectrum {
  const bins = Array.from({ length: 11 }, (_, i) => i);
  const fill = bins.map(() => level);
  return { freq_bins: bins, psd_x: fill, psd_y: fill, psd_z: fill, psd_sum: fill };
}

beforeEach(() => {
  chart = undefined as never;
});

describe("EnergyStrip", () => {
  it("draws one line per chip with the energy in a 2 Hz band by default", () => {
    const chips = [
      { chip: "a", color: "#fff", spectrum: flat(2) },
      { chip: "b", color: "#000", spectrum: flat(1) },
    ];
    render(<EnergyStrip chips={chips} frequencyOfInterest={5} now={() => 100} />);
    expect(chart.option.series.map((s) => s.name)).toEqual(["a", "b"]);
    expect(chart.option.series[0].data.at(-1)).toEqual([0, 10]);
    expect(chart.option.series[1].data.at(-1)).toEqual([0, 5]);
    expect(chart.option.yAxis.name).toBe("Energy 3.0 to 7.0 Hz");
  });

  it("changes the plotted values and the label when the band is widened", () => {
    render(
      <EnergyStrip chips={[{ chip: "a", color: "#fff", spectrum: flat(2) }]} frequencyOfInterest={5} now={() => 100} />,
    );
    fireEvent.change(screen.getByLabelText("Band half width (Hz)"), { target: { value: "4" } });
    expect(chart.option.series[0].data.at(-1)).toEqual([0, 18]);
    expect(chart.option.yAxis.name).toBe("Energy 1.0 to 9.0 Hz");
  });

  it("starts the history again when the frequency of interest changes", () => {
    let t = 100;
    const chips = [{ chip: "a", color: "#fff", spectrum: flat(2) }];
    const { rerender } = render(<EnergyStrip chips={chips} frequencyOfInterest={5} now={() => t} />);
    t = 101;
    rerender(<EnergyStrip chips={[{ ...chips[0], spectrum: flat(3) }]} frequencyOfInterest={5} now={() => t} />);
    expect(chart.option.series[0].data).toHaveLength(2);
    rerender(<EnergyStrip chips={chips} frequencyOfInterest={6} now={() => t} />);
    expect(chart.option.series[0].data).toHaveLength(1);
  });
});
