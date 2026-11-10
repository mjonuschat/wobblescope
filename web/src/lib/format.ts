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

/** An RFC 3339 time in the viewer's locale. */
export function formatTime(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
}
