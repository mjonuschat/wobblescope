import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { dark, light, resolve } from "@/test/tokens";
import { useChartTheme } from "@/ui-kit/chartTheme";

beforeEach(() => {
  // jsdom loads no stylesheet, so answer a token read from the CSS files by the root's `dark` class.
  vi.spyOn(window, "getComputedStyle").mockImplementation(
    () =>
      ({
        getPropertyValue: (token: string) =>
          resolve(document.documentElement.classList.contains("dark") ? dark : light, token),
      }) as CSSStyleDeclaration,
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  document.documentElement.classList.remove("dark");
});

describe("useChartTheme", () => {
  it("reads the tokens again when the theme changes, so a chart redraws in the new colors", async () => {
    const { result } = renderHook(() => useChartTheme());
    const before = result.current;
    act(() => document.documentElement.classList.add("dark"));
    await waitFor(() => expect(result.current).not.toBe(before));
    expect(result.current.text).not.toBe(before.text);
    expect(result.current.surface).not.toBe(before.surface);
  });

  it("keeps the same theme object while the theme is unchanged", () => {
    const { result, rerender } = renderHook(() => useChartTheme());
    const first = result.current;
    rerender();
    expect(result.current).toBe(first);
  });
});
