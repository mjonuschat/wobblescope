import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { AnalysisResult } from "@/analysis/protocol";
import { analysis, twoAxisMeta } from "@/test/fixtures";

import { AxisPairResults } from "./AxisPairResults";

const options = vi.hoisted(() => [] as { yAxis?: { max?: number } }[]);
vi.mock("@/components/charts/EChart", () => ({
  EChart: ({ option, label }: { option: { yAxis?: { max?: number } }; label: string }) => {
    if (label.startsWith("All chips")) options.push(option);
    return null;
  },
}));

function result(peak: number): AnalysisResult {
  const a = analysis();
  if (a.summed) a.summed = { ...a.summed, psd: [1, peak, 1] };
  return { analysis: a, snippets: { worstCase: null, worstCasePerformance: null, perChip: [null, null] } };
}

describe("AxisPairResults", () => {
  it("gives the two all-chips charts one vibration scale, the larger of the two", () => {
    options.length = 0;
    render(
      <QueryClientProvider client={new QueryClient()}>
        <AxisPairResults
          columns={[
            { axis: "X", result: result(100), source: { sessionId: "s", axis: "X" } },
            { axis: "Y", result: result(400), source: { sessionId: "s", axis: "Y" } },
          ]}
          meta={twoAxisMeta("s")}
          overrides={[]}
          readOnly
        />
      </QueryClientProvider>,
    );
    const tab = screen.getByRole("tab", { name: "All chips" });
    fireEvent.mouseDown(tab);
    fireEvent.click(tab);
    const maxes = options.slice(-2).map((o) => o.yAxis?.max);
    expect(maxes).toEqual([420, 420]);
  });
});
