import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const src = path.resolve(import.meta.dirname, "..");
const SKIP_DIRS = new Set(["wasm", "generated", "test"]);
const PALETTE =
  /\b(?:text|bg|border|ring|fill|stroke|from|to|via|divide|outline|shadow)-(?:red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose|slate|gray|zinc|neutral|stone)-\d{2,3}\b|\b(?:text|bg|border)-(?:white|black)\b/g;

function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = path.join(dir, name);
    if (statSync(full).isDirectory()) return SKIP_DIRS.has(name) ? [] : sources(full);
    return /\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name) ? [full] : [];
  });
}

describe("theme tokens", () => {
  it("are the only colors in components: no raw Tailwind palette class", () => {
    const found = sources(src).flatMap((file) =>
      [...readFileSync(file, "utf8").matchAll(PALETTE)].map((m) => `${path.relative(src, file)}: ${m[0]}`),
    );
    // components/ui holds the shadcn primitives, which use tokens plus a few literal white and black overlays.
    expect(found.filter((line) => !line.startsWith("components/ui/"))).toEqual([]);
  });
});
