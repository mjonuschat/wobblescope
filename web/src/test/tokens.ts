import { readFileSync } from "node:fs";
import path from "node:path";

const read = (file: string) => readFileSync(path.resolve(import.meta.dirname, file), "utf8");
const css = `${read("../brand.css")}\n${read("../ui-kit/tokens.css")}`;

function declarations(selector: ":root" | ".dark"): Record<string, string> {
  const out: Record<string, string> = {};
  const blocks = css.matchAll(/(:root|\.dark)\s*\{([^}]*)\}/g);
  for (const [, sel, body] of blocks) {
    if (sel !== selector) continue;
    for (const [, name, value] of body.matchAll(/(--[\w-]+):\s*([^;]+);/g)) out[name] = value.trim();
  }
  return out;
}

export const light = declarations(":root");
export const dark = { ...light, ...declarations(".dark") };

export function resolve(vars: Record<string, string>, name: string): string {
  let value = vars[name];
  for (let i = 0; i < 5 && value?.startsWith("var("); i++) value = vars[value.slice(4, -1)];
  if (!value) throw new Error(`no value for ${name}`);
  return value;
}
