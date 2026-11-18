import { readFileSync } from "node:fs";
import path from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";

const html = readFileSync(path.resolve(import.meta.dirname, "../../index.html"), "utf8");
const script = /<script>([\s\S]*?)<\/script>/.exec(html)?.[1] ?? "";

function run(stored: string | null, prefersDark: boolean, storageThrows = false) {
  document.documentElement.className = "";
  document.documentElement.style.colorScheme = "";
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    if (storageThrows) throw new Error("denied");
    return stored;
  });
  window.matchMedia = ((query: string) => ({
    matches: prefersDark,
    media: query,
  })) as unknown as typeof window.matchMedia;
  new Function(script)();
}

afterEach(() => vi.restoreAllMocks());

describe("the inline theme script in index.html", () => {
  it("exists and uses the key of the ThemeProvider in main.tsx", () => {
    expect(script).toContain('"wobblescope-theme"');
    expect(readFileSync(path.resolve(import.meta.dirname, "../main.tsx"), "utf8")).toContain(
      'storageKey="wobblescope-theme"',
    );
  });

  it("applies a stored mode over the system preference", () => {
    run("light", true);
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    run("dark", false);
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.style.colorScheme).toBe("dark");
  });

  it("falls back to the system preference when nothing is stored", () => {
    run(null, true);
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    run(null, false);
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("does not throw when localStorage is unavailable", () => {
    expect(() => run(null, true, true)).not.toThrow();
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });
});
