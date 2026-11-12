import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AnalysisResult } from "@/analysis/protocol";
import type { Analysis } from "@/lib/generated/Analysis";
import { analysis, candidate, recommendation } from "@/test/fixtures";
import { renderRoute } from "@/test/render";
import { resultDetail } from "@/test/results";
import { stubApiWithStatus as stubApi } from "@/test/status";

const fake = vi.hoisted(() => ({ analyze: vi.fn(), forget: vi.fn() }));

vi.mock("@/analysis/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/analysis/client")>()),
  getAnalysisClient: () => fake,
}));
vi.mock("@/components/charts/EChart", () => ({
  EChart: ({ label }: { label: string }) => <div role="img" aria-label={label} />,
}));

function fresh(a: Analysis): AnalysisResult {
  return {
    analysis: a,
    snippets: { worstCase: "fresh snippet", worstCasePerformance: null, perChip: [["fresh snippet"], null] },
  };
}

function serve(detail = resultDetail()) {
  return stubApi(({ url }) =>
    url.pathname.endsWith("/results/r1")
      ? { body: detail }
      : { status: 404, body: { error: "not_found", message: "no such result" } },
  );
}

beforeEach(() => {
  fake.analyze.mockReset();
});

function pending(_job: unknown, signal?: AbortSignal) {
  return new Promise((_resolve, reject) => {
    signal?.addEventListener("abort", () => reject(new DOMException("cancelled", "AbortError")));
  });
}

describe("the result page", () => {
  it("renders the saved snapshot and snippet without re-running anything first", async () => {
    fake.analyze.mockImplementation(pending);
    serve();
    renderRoute("/results/r1");
    expect((await screen.findAllByText(/shaper_type_x: mzv/)).length).toBeGreaterThan(0);
    expect(screen.getByRole("region", { name: "Recommended Input Shaper" })).toBeTruthy();
    expect(screen.getByText("after belts")).toBeTruthy();
    expect(screen.queryByText(/same answer/)).toBeNull();
    expect(screen.queryByText("fresh snippet")).toBeNull();
  });

  it("re-analyses from the result route with the saved parameters", async () => {
    fake.analyze.mockImplementation(pending);
    const detail = resultDetail();
    serve(detail);
    renderRoute("/results/r1");
    await waitFor(() => expect(fake.analyze).toHaveBeenCalled());
    const job = fake.analyze.mock.calls[0][0];
    expect(job.sessionId).toBe("result:r1");
    expect(job.resultId).toBe("r1");
    expect(job.params).toEqual(detail.record.fit_params);
    expect(job.overrides).toEqual(detail.record.overrides);
    expect(job.axis).toBe("X");
  });

  it("disables override checkboxes", async () => {
    const detail = resultDetail();
    detail.record.axes[0].analysis.eligibility.decisions = [
      { chip: "b", point: 0, decision: "Excluded", reasons: ["degraded"], dropped: null },
    ];
    fake.analyze.mockImplementation(pending);
    serve(detail);
    renderRoute("/results/r1");
    const box = await screen.findByRole("checkbox", { name: /Include b anyway/ });
    expect(box.hasAttribute("disabled") || box.getAttribute("aria-disabled") === "true").toBe(true);
  });

  it("warns when the saved core version differs from the re-analysis", async () => {
    fake.analyze.mockResolvedValue(fresh(analysis()));
    serve(resultDetail({ coreVersion: "0.0.9" }));
    renderRoute("/results/r1");
    expect(await screen.findByText(/saved with core 0\.0\.9/i)).toBeTruthy();
    expect(screen.getByText(/re-analysed with core 0\.1\.0/i)).toBeTruthy();
  });

  it("warns when re-analysis changes the answer", async () => {
    const changed = analysis();
    changed.worst_case = recommendation(candidate("mzv", 48.2, { max_accel: 4200 }));
    fake.analyze.mockResolvedValue(fresh(changed));
    serve();
    renderRoute("/results/r1");
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("X worst case: max_accel 4500 -> 4200");
    expect(screen.getAllByText(/shaper_type_x: mzv/).length).toBeGreaterThan(0);
    expect(screen.queryByText("fresh snippet")).toBeNull();
  });

  it("says the data is intact when it matches", async () => {
    fake.analyze.mockResolvedValue(fresh(analysis()));
    serve();
    renderRoute("/results/r1");
    expect(await screen.findByText(/The saved data is intact/)).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("says so when the re-analysis cannot run, and still shows the snapshot", async () => {
    fake.analyze.mockRejectedValue(new Error("sample file is damaged"));
    serve();
    renderRoute("/results/r1");
    expect(await screen.findByText(/sample file is damaged/)).toBeTruthy();
    expect(screen.getAllByText(/shaper_type_x: mzv/).length).toBeGreaterThan(0);
  });

  it("shows the daemon\u2019s error for an unusable result", async () => {
    stubApi(() => ({
      status: 422,
      body: { error: "result_unusable", message: "the manifest does not match" },
    }));
    renderRoute("/results/r1");
    expect(await screen.findByText("the manifest does not match")).toBeTruthy();
    expect(fake.analyze).not.toHaveBeenCalled();
  });

  it("switches between the axes of a result", async () => {
    const detail = resultDetail();
    detail.record.axes.push({
      ...detail.record.axes[0],
      axis: "Y",
      snippets: { worst_case: "y snippet", per_chip: [["y snippet"], null], worst_case_performance: null },
    });
    fake.analyze.mockImplementation(pending);
    serve(detail);
    renderRoute("/results/r1");
    const tab = await screen.findByRole("tab", { name: "Axis Y" });
    fireEvent.mouseDown(tab);
    fireEvent.click(tab);
    expect(await screen.findAllByText(/y snippet/)).toBeTruthy();
  });
});
