import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ThemeProvider, useTheme } from "@/ui-kit/ThemeProvider";

const KEY = "test-theme";

function fakeMedia(initial: boolean) {
  let matches = initial;
  const listeners = new Set<() => void>();
  const original = window.matchMedia;
  window.matchMedia = ((query: string) => ({
    get matches() {
      return matches;
    },
    media: query,
    addEventListener: (_: string, fn: () => void) => listeners.add(fn),
    removeEventListener: (_: string, fn: () => void) => listeners.delete(fn),
  })) as unknown as typeof window.matchMedia;
  return {
    set(next: boolean) {
      matches = next;
      for (const fn of listeners) fn();
    },
    restore() {
      window.matchMedia = original;
    },
  };
}

function Probe() {
  const { mode, resolved, setMode } = useTheme();
  return (
    <>
      <p data-testid="state">{`${mode}/${resolved}`}</p>
      <button type="button" onClick={() => setMode("light")}>
        light
      </button>
    </>
  );
}

let media: ReturnType<typeof fakeMedia>;
beforeEach(() => {
  window.localStorage.clear();
  document.documentElement.className = "";
});
afterEach(() => media.restore());

function renderProvider() {
  render(
    <ThemeProvider storageKey={KEY}>
      <Probe />
    </ThemeProvider>,
  );
}

describe("ThemeProvider", () => {
  it("starts on system and resolves it from the system preference", () => {
    media = fakeMedia(true);
    renderProvider();
    expect(screen.getByTestId("state").textContent).toBe("system/dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("follows a change of the system preference while on system", () => {
    media = fakeMedia(true);
    renderProvider();
    act(() => media.set(false));
    expect(screen.getByTestId("state").textContent).toBe("system/light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("ignores the system preference once a mode is chosen, and stores the choice", () => {
    media = fakeMedia(true);
    renderProvider();
    act(() => screen.getByText("light").click());
    expect(screen.getByTestId("state").textContent).toBe("light/light");
    expect(window.localStorage.getItem(KEY)).toBe("light");
    act(() => media.set(true));
    expect(screen.getByTestId("state").textContent).toBe("light/light");
  });

  it("starts from the stored mode", () => {
    media = fakeMedia(false);
    window.localStorage.setItem(KEY, "dark");
    renderProvider();
    expect(screen.getByTestId("state").textContent).toBe("dark/dark");
  });
});
