import { QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { createQueryClient } from "@/lib/api/queries";
import type { ChipAnalysis } from "@/lib/generated/ChipAnalysis";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import { analysis, spectrogram, twoAxisMeta } from "@/test/fixtures";

import { SpectrogramPanel } from "./SpectrogramPanel";

const fake = vi.hoisted(() => ({ analyze: vi.fn(), spectrogram: vi.fn(), forget: vi.fn(), prefetch: vi.fn() }));

vi.mock("@/analysis/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/analysis/client")>()),
  getAnalysisClient: () => fake,
}));
vi.mock("@/components/charts/EChart", () => ({
  EChart: ({ label }: { label: string }) => <div role="img" aria-label={label} />,
}));

const found = { peaks: [{ freq: 1.5, level: 4, above_effect: true }], detect_threshold: 1, effect_threshold: 3 };

function chipOf(over: Partial<ChipAnalysis> = {}): ChipAnalysis {
  return { ...analysis().per_chip[0], chip: "adxl345", peaks: [[0, found]], ...over };
}

function twoPointMeta(): SessionMeta {
  const meta = twoAxisMeta();
  meta.probe_points = [{ position: [10, 10, 20] }, { position: [100, 100, 20] }];
  meta.segments.push({
    ...meta.segments[0],
    point: 1,
    files: [{ ...meta.segments[0].files[0], sample_file: "c0-s2.zst" }],
  });
  return meta;
}

function renderPanel(
  chip = chipOf(),
  meta = twoAxisMeta(),
  source: { sessionId: string; resultId?: string } = { sessionId: "s1" },
) {
  const client = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return render(<SpectrogramPanel chip={chip} meta={meta} source={{ ...source, axis: "X" }} maxFreq={200} />, {
    wrapper,
  });
}

beforeEach(() => {
  fake.spectrogram.mockReset();
});

describe("the spectrogram panel", () => {
  it("makes the map as soon as it is shown, with no button to press", async () => {
    fake.spectrogram.mockResolvedValue(spectrogram());
    renderPanel();
    expect(screen.getByRole("region", { name: "Spectrogram" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Show spectrogram" })).toBeNull();
    expect(await screen.findByRole("img", { name: "Spectrogram of adxl345" })).toBeTruthy();
    expect(fake.spectrogram).toHaveBeenCalledTimes(1);
    expect(fake.spectrogram.mock.calls[0][0]).toMatchObject({
      sessionId: "s1",
      axis: "X",
      chip: "adxl345",
      point: 0,
      maxFreq: 200,
    });
  });

  it("shows a wait message while it loads and the error when it fails", async () => {
    fake.spectrogram.mockReturnValue(new Promise(() => {}));
    const first = renderPanel();
    expect(await screen.findByText(/Making the spectrogram/)).toBeTruthy();
    first.unmount();

    fake.spectrogram.mockRejectedValue(new Error("sample file c0-s0.zst holds 999 samples, metadata says 1000"));
    renderPanel();
    expect((await screen.findByRole("alert")).textContent).toContain("holds 999 samples");
  });

  it("serves a second view of the same recording from the cache", async () => {
    fake.spectrogram.mockResolvedValue(spectrogram());
    const client = createQueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const element = (
      <SpectrogramPanel chip={chipOf()} meta={twoAxisMeta()} source={{ sessionId: "s1", axis: "X" }} maxFreq={200} />
    );
    const first = render(element, { wrapper });
    await screen.findByRole("img");
    first.unmount();
    render(element, { wrapper });
    expect(await screen.findByRole("img")).toBeTruthy();
    expect(fake.spectrogram).toHaveBeenCalledTimes(1);
  });

  it("offers a point selector only for a chip with several probe points", async () => {
    fake.spectrogram.mockResolvedValue(spectrogram());
    const single = renderPanel();
    await screen.findByRole("img");
    expect(screen.queryByRole("group", { name: "Probe point" })).toBeNull();
    single.unmount();

    renderPanel(chipOf(), twoPointMeta());
    const group = await screen.findByRole("group", { name: "Probe point" });
    fireEvent.click(within(group).getAllByRole("button")[1]);
    await waitFor(() => expect(fake.spectrogram.mock.calls.at(-1)?.[0]).toMatchObject({ point: 1 }));
  });

  it("lists a failed recording as a point too, next to a good one", async () => {
    fake.spectrogram.mockReturnValue(new Promise(() => {}));
    // The chip's analysis only has point 0, but the metadata also records point 1.
    renderPanel(chipOf(), twoPointMeta());
    expect(
      await within(await screen.findByRole("group", { name: "Probe point" })).findAllByRole("button"),
    ).toHaveLength(2);
  });

  it("a chip with no successful PSD still has its point to load and shows the error", async () => {
    fake.spectrogram.mockRejectedValue(new Error("c0-s0.zst is damaged"));
    renderPanel(chipOf({ datasets: [], peaks: [] }));
    expect((await screen.findByRole("alert")).textContent).toContain("damaged");
    expect(fake.spectrogram.mock.calls[0][0]).toMatchObject({ chip: "adxl345", point: 0 });
  });

  it("says so when the chip has no recording on this axis, and fetches nothing", () => {
    renderPanel(chipOf({ chip: "not on this axis" }));
    expect(screen.getByText(/no recording/i)).toBeTruthy();
    expect(fake.spectrogram).not.toHaveBeenCalled();
  });
});
