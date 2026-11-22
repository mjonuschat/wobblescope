export type Rgba = readonly [r: number, g: number, b: number, a: number];

/** Parses `oklch(L C H)` or `oklch(L C H / A)`, the only color form the theme tokens use. */
export function parseOklch(value: string): Rgba | null {
  const m = /^oklch\(\s*([\d.]+)(%?)\s+([\d.]+)\s+([\d.]+)(?:deg)?\s*(?:\/\s*([\d.]+)(%?))?\s*\)$/.exec(value.trim());
  if (!m) return null;
  const L = Number(m[1]) / (m[2] ? 100 : 1);
  const C = Number(m[3]);
  const h = (Number(m[4]) * Math.PI) / 180;
  const alpha = m[5] === undefined ? 1 : Number(m[5]) / (m[6] ? 100 : 1);
  const a = C * Math.cos(h);
  const b = C * Math.sin(h);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const mm = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const linear = [
    4.0767416621 * l - 3.3077115913 * mm + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * mm - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * mm + 1.707614701 * s,
  ];
  const [r, g, bl] = linear.map((v) => {
    const c = Math.min(1, Math.max(0, v));
    return Math.round(255 * (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055));
  });
  return [r, g, bl, alpha];
}

export function toCss([r, g, b, a]: Rgba): string {
  return a >= 1 ? `rgb(${r}, ${g}, ${b})` : `rgba(${r}, ${g}, ${b}, ${Number(a.toFixed(3))})`;
}

function parseCss(value: string): Rgba | null {
  const m = /^rgba?\(\s*(\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?\s*\)$/.exec(value.trim());
  return m ? [Number(m[1]), Number(m[2]), Number(m[3]), m[4] === undefined ? 1 : Number(m[4])] : null;
}

/** The color with its opacity set to `alpha`. Reads `rgb()` and `rgba()` strings from this module. */
export function withAlpha(color: string, alpha: number): string {
  const rgba = parseCss(color);
  return rgba ? toCss([rgba[0], rgba[1], rgba[2], alpha]) : color;
}

export function mixColors(from: string, to: string, t: number): string {
  const a = parseCss(from);
  const b = parseCss(to);
  if (!a || !b) return from;
  return toCss(
    [0, 1, 2, 3].map((i) =>
      i === 3 ? a[3] + (b[3] - a[3]) * t : Math.round(a[i] + (b[i] - a[i]) * t),
    ) as unknown as Rgba,
  );
}

function luminance([r, g, b]: Rgba): number {
  const channel = (v: number) => {
    const c = v / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

const WHITE: Rgba = [255, 255, 255, 1];
const INK: Rgba = [26, 28, 33, 1];

export function readableOn(fill: string): string {
  const rgba = parseCss(fill);
  if (!rgba) return toCss(WHITE);
  const l = luminance(rgba);
  const against = (text: Rgba) => (Math.max(l, luminance(text)) + 0.05) / (Math.min(l, luminance(text)) + 0.05);
  return toCss(against(INK) >= against(WHITE) ? INK : WHITE);
}
