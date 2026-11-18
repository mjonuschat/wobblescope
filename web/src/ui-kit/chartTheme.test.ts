import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

import { dark, light, resolve } from "@/test/tokens";
import { chartThemeFrom, DEFAULT_CHART_THEME, FALLBACK_TOKENS } from "@/ui-kit/chartTheme";
import { parseOklch, toCss } from "@/ui-kit/color";

const reader = (vars: Record<string, string>) => (token: string) => resolve(vars, token);

describe("chartThemeFrom", () => {
  it("keeps the fallback tokens in step with the dark theme in tokens.css", () => {
    for (const [token, value] of Object.entries(FALLBACK_TOKENS)) {
      expect(value, token).toBe(resolve(dark, token));
    }
  });

  it("reads the dark tokens as the default theme", () => {
    expect(chartThemeFrom(reader(dark))).toEqual({
      ...DEFAULT_CHART_THEME,
    });
  });

  it("gives the light theme a dark text color and the darker accent ink", () => {
    const lightTheme = chartThemeFrom(reader(light));
    const darkTheme = chartThemeFrom(reader(dark));
    expect(lightTheme.text).not.toBe(darkTheme.text);
    expect(lightTheme.accent).toBe(toCss(parseOklch(resolve(light, "--brand-ink"))!));
    expect(darkTheme.accent).toBe(toCss(parseOklch(resolve(dark, "--brand"))!));
  });

  it("falls back to the dark value for a token the page does not define", () => {
    expect(chartThemeFrom(() => "").text).toBe(DEFAULT_CHART_THEME.text);
  });
});

describe("color helpers", () => {
  it("reads oklch with and without alpha and rejects other forms", () => {
    expect(parseOklch("oklch(1 0 0)")).toEqual([255, 255, 255, 1]);
    expect(parseOklch("oklch(0 0 0 / 50%)")).toEqual([0, 0, 0, 0.5]);
    expect(parseOklch("#fff")).toBeNull();
  });

  it("picks readable text for a fill", () => {
    const theme = DEFAULT_CHART_THEME;
    expect(theme.onFill("rgb(255, 255, 255)")).toBe("rgb(26, 28, 33)");
    expect(theme.onFill("rgb(20, 20, 20)")).toBe("rgb(255, 255, 255)");
  });

  it("sets the opacity of a color", () => {
    expect(DEFAULT_CHART_THEME.alpha("rgb(10, 20, 30)", 0.5)).toBe("rgba(10, 20, 30, 0.5)");
  });
});

describe("chart colors", () => {
  const src = path.resolve(import.meta.dirname, "..");
  // Colors that encode data or sit on a heatmap, and the theme itself.
  const ALLOWED = new Set(["ui-kit/chartTheme.ts", "ui-kit/color.ts", "components/charts/colormaps.ts"]);
  const LITERAL = /#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/;

  function sources(dir: string): string[] {
    return readdirSync(dir).flatMap((name) => {
      const full = path.join(dir, name);
      if (statSync(full).isDirectory()) return ["wasm", "generated", "test"].includes(name) ? [] : sources(full);
      return /(Option|options|trace|colors|AxesMapPlot)\.(ts|tsx)$/.test(name) ? [full] : [];
    });
  }

  it("come from the theme: no color literal in the chart builders", () => {
    const found = sources(src)
      .filter((file) => !ALLOWED.has(path.relative(src, file)))
      .filter((file) => LITERAL.test(readFileSync(file, "utf8").replace(/\/\*[\s\S]*?\*\/|\/\/.*$/gm, "")))
      .map((file) => path.relative(src, file));
    // AxesMapPlot keeps Shake&Tune's three chip axis colors (see shaketune-deviations.md).
    expect(found.filter((f) => f !== "pages/session/AxesMapPlot.tsx")).toEqual([]);
  });
});
