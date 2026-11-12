import type { Axis } from "@/lib/generated/Axis";

export interface FileNameReading {
  axis?: Axis;
  point?: [number, number, number];
  stamp?: Date;
  stampToken?: string;
}

const PREFIX = "raw_data_";
const STAMP = /(?<!\d)(\d{4})(\d{2})(\d{2})_(\d{2})(\d{2})(\d{2})(?!\d)/;
const THREE_DECIMALS = /^-?\d+\.\d{3}$/;

function stem(name: string): string {
  const base = name.slice(name.lastIndexOf("/") + 1);
  return base.replace(/\.csv$/i, "");
}

/** The text after `raw_data_<axis>_`, or undefined when the name has no x or y axis. */
function afterAxis(name: string): { axis: Axis; rest: string } | undefined {
  const s = stem(name);
  if (!s.startsWith(PREFIX)) return undefined;
  const tail = s.slice(PREFIX.length);
  const axisChar = tail.split("_")[0];
  if (axisChar !== "x" && axisChar !== "y") return undefined;
  return { axis: axisChar === "x" ? "X" : "Y", rest: tail.slice(axisChar.length + 1) };
}

function readStamp(text: string): { stamp: Date; stampToken: string } | undefined {
  const m = STAMP.exec(text);
  if (!m) return undefined;
  const [token, y, mo, d, h, mi, s] = m;
  const [year, month, day, hour, minute, second] = [y, mo, d, h, mi, s].map(Number);
  const stamp = new Date(year, month - 1, day, hour, minute, second);
  const valid =
    stamp.getFullYear() === year &&
    stamp.getMonth() === month - 1 &&
    stamp.getDate() === day &&
    stamp.getHours() === hour &&
    stamp.getMinutes() === minute &&
    stamp.getSeconds() === second;
  return valid ? { stamp, stampToken: token } : undefined;
}

/** Reads the axis, probe point and time from a Klipper raw file name, by its base name. */
export function readFileName(name: string): FileNameReading {
  const parts = afterAxis(name);
  if (!parts) return {};
  const reading: FileNameReading = { axis: parts.axis };
  const tokens = parts.rest.split("_");
  for (let i = 0; i + 3 <= tokens.length; i++) {
    const w = tokens.slice(i, i + 3);
    if (w.every((t) => THREE_DECIMALS.test(t))) {
      reading.point = [Number(w[0]), Number(w[1]), Number(w[2])];
      break;
    }
  }
  const stamp = readStamp(stem(name));
  if (stamp) Object.assign(reading, stamp);
  return reading;
}

/** The longest known chip name that starts the name after the axis and ends at `_`, `.` or the end of the stem. */
export function guessChip(name: string, known: string[]): string | undefined {
  const parts = afterAxis(name);
  if (!parts) return undefined;
  let best: string | undefined;
  for (const chip of known) {
    const key = chip.replaceAll(" ", "_");
    if (!key || !parts.rest.startsWith(key)) continue;
    const next = parts.rest.charAt(key.length);
    if (next !== "" && next !== "_" && next !== ".") continue;
    if (best === undefined || chip.length > best.length) best = chip;
  }
  return best;
}

/** Parses `x, y, z`. Empty text is no point; anything else that is not three finite numbers is invalid. */
export function parsePointText(text: string): [number, number, number] | null | "invalid" {
  if (text.trim() === "") return null;
  const fields = text.split(",").map((f) => f.trim());
  if (fields.length !== 3 || fields.some((f) => f === "")) return "invalid";
  const nums = fields.map(Number);
  if (nums.some((n) => !Number.isFinite(n))) return "invalid";
  return [nums[0], nums[1], nums[2]];
}

export function isCsv(name: string): boolean {
  return name.toLowerCase().endsWith(".csv");
}
