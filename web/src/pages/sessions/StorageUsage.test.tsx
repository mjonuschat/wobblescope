import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { usage } from "@/test/summaries";

import { StorageUsage } from "./StorageUsage";

describe("StorageUsage", () => {
  it("shows saved results apart from sessions", () => {
    render(<StorageUsage usage={{ ...usage, result_count: 3, result_bytes: 2 * 1024 ** 2 }} />);
    expect(screen.getByText("Saved results: 3, 2 MiB")).toBeTruthy();
    expect(screen.getByText("12 sessions")).toBeTruthy();
  });
});
