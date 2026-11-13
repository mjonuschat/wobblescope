const UNITS = ["B", "KiB", "MiB", "GiB", "TiB"];

/** Bytes as a short binary-unit figure: 0 B, 1.5 KiB, 100 MiB. */
export function formatBytes(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  if (unit === 0) return `${value} B`;
  return `${value.toFixed(1).replace(/\.0$/, "")} ${UNITS[unit]}`;
}

/** A frequency to the 0.1 Hz core's snippet uses, so float noise such as 52.60000000000019 never shows. */
export function formatHz(hz: number): string {
  return hz.toFixed(1);
}

/** A maximum acceleration to the nearest 100 mm/s², as Klipper reports it. The figure rests on a smoothing heuristic and has no finer precision. */
export function formatAccel(accel: number): string {
  return `${Math.round(accel / 100) * 100} mm/s²`;
}

export function formatTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  const two = (n: number) => String(n).padStart(2, "0");
  const day = `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())}`;
  return `${day} ${two(date.getHours())}:${two(date.getMinutes())}:${two(date.getSeconds())}`;
}

/**
 * A chip's Klipper section name for display: "lis2dw12 tool" becomes "LIS2DW12 Tool". A word that
 * mixes letters and digits is a part number, so it is upper-cased. Never use it for a request or key.
 */
export function chipLabel(name: string): string {
  return name
    .split(/[_\s]+/)
    .filter((word) => word !== "")
    .map((word) =>
      /\d/.test(word) && /[a-z]/i.test(word) ? word.toUpperCase() : word.charAt(0).toUpperCase() + word.slice(1),
    )
    .join(" ");
}
