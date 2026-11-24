import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { analysis } from "@/test/fixtures";

import { ChipResult } from "./ChipResult";

const options = vi.hoisted(() => [] as unknown[]);
vi.mock("@/components/charts/EChart", () => ({
  EChart: ({ option }: { option: unknown }) => {
    options.push(option);
    return null;
  },
}));

describe("ChipResult", () => {
  it("gives the chart the same option when it renders again with the same data, so legend choices stay", () => {
    options.length = 0;
    const chip = analysis().per_chip[0];
    const props = { chip, switcher: null, snippets: null, maxFreq: 200, decisions: [] };
    const { rerender } = render(<ChipResult {...props} />);
    rerender(<ChipResult {...props} />);
    expect(options.length).toBeGreaterThanOrEqual(2);
    expect(new Set(options).size).toBe(1);
  });
});
