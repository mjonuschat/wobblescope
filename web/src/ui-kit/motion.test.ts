import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const src = path.resolve(import.meta.dirname, "..");
const BANNED = /\btransition-all\b|\banimate-(?:pulse|ping|bounce)\b|\bhover:scale-/g;

function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = path.join(dir, name);
    if (statSync(full).isDirectory()) return ["wasm", "generated"].includes(name) ? [] : sources(full);
    return /\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name) ? [full] : [];
  });
}

describe("motion", () => {
  it("never animates everything, loops for no reason or scales on hover", () => {
    const found = sources(src).flatMap((file) =>
      [...readFileSync(file, "utf8").matchAll(BANNED)].map((m) => `${path.relative(src, file)}: ${m[0]}`),
    );
    expect(found).toEqual([]);
  });

  it("stops animation and transitions when the user asks for reduced motion", () => {
    const css = readFileSync(path.resolve(src, "ui-kit/tokens.css"), "utf8");
    expect(css).toContain("@media (prefers-reduced-motion: reduce)");
    expect(css).toMatch(/transition-duration: 0\.01ms/);
  });
});
