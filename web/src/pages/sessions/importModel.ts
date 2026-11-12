import type { Axis } from "@/lib/generated/Axis";
import { guessChip, isCsv, parsePointText, readFileName } from "@/lib/importFiles";
import { formatBytes } from "@/lib/format";

export const MAX_IMPORT_BYTES = 64 * 1024 * 1024;
/** The daemon refuses a request with more files than this. */
export const MAX_IMPORT_FILES = 16;

export interface ImportEntry {
  id: string;
  file: File;
  chip: string;
  chipGuessed: boolean;
  axis: Axis | "";
  point: string;
  stampToken: string | null;
}

export interface ImportGroupState {
  id: string;
  createdAt: Date;
  entries: ImportEntry[];
  confirmed: boolean;
}

export type ImportState = ImportGroupState[];

export interface AddContext {
  known: string[];
  defaults: { x: string[]; y: string[] };
  now: Date;
}

export interface ImportProblem {
  entryId: string | null;
  text: string;
}

let counter = 0;
function nextId(prefix: string): string {
  counter += 1;
  return `${prefix}-${counter}`;
}

function formatPoint(point: [number, number, number]): string {
  return point.map((n) => n.toFixed(3)).join(", ");
}

function sameFile(a: File, b: File): boolean {
  return a.name === b.name && a.size === b.size && a.lastModified === b.lastModified;
}

/** The chip named in the file name, else the printer's default for the axis, else the first known chip. */
function guessedChip(fileName: string, axis: Axis | "", context: AddContext): string {
  const named = guessChip(fileName, context.known);
  const byAxis = axis === "X" ? context.defaults.x[0] : axis === "Y" ? context.defaults.y[0] : undefined;
  return named ?? byAxis ?? context.known[0] ?? "";
}

function makeEntry(file: File, context: AddContext): { entry: ImportEntry; stamp: Date | undefined } {
  const reading = readFileName(file.name);
  return {
    stamp: reading.stamp,
    entry: {
      id: nextId("entry"),
      file,
      chip: guessedChip(file.name, reading.axis ?? "", context),
      chipGuessed: true,
      axis: reading.axis ?? "",
      point: reading.point ? formatPoint(reading.point) : "",
      stampToken: reading.stampToken ?? null,
    },
  };
}

export function addFiles(
  state: ImportState,
  files: File[],
  context: AddContext,
): { state: ImportState; refused: { name: string; reason: string }[] } {
  const refused: { name: string; reason: string }[] = [];
  let next = state;
  for (const file of files) {
    if (!isCsv(file.name)) {
      refused.push({ name: file.name, reason: "Only .csv files can be imported." });
      continue;
    }
    if (next.some((g) => g.entries.some((e) => sameFile(e.file, file)))) {
      refused.push({ name: file.name, reason: "This file is already in the list." });
      continue;
    }
    const { entry, stamp } = makeEntry(file, context);
    const target = entry.stampToken
      ? next.find((g) => g.entries.some((e) => e.stampToken === entry.stampToken))
      : undefined;
    if (target) {
      next = next.map((g) => (g.id === target.id ? { ...g, entries: [...g.entries, entry], confirmed: false } : g));
    } else {
      next = [...next, { id: nextId("group"), createdAt: stamp ?? context.now, entries: [entry], confirmed: false }];
    }
  }
  return { state: next, refused };
}

/** Guesses again the chip of every file whose chip the user has not set, for when the chip list arrives late. */
export function reguessChips(state: ImportState, context: AddContext): ImportState {
  let changed = false;
  const next = state.map((group) => {
    let groupChanged = false;
    const entries = group.entries.map((entry) => {
      if (!entry.chipGuessed) return entry;
      const chip = guessedChip(entry.file.name, entry.axis, context);
      if (chip === entry.chip) return entry;
      groupChanged = true;
      return { ...entry, chip };
    });
    if (!groupChanged) return group;
    changed = true;
    return { ...group, entries };
  });
  return changed ? next : state;
}

export function moveEntry(state: ImportState, entryId: string, groupId: string | "new"): ImportState {
  const source = state.find((g) => g.entries.some((e) => e.id === entryId));
  if (!source || source.id === groupId) return state;
  const entry = source.entries.find((e) => e.id === entryId) as ImportEntry;
  const without = (g: ImportGroupState): ImportGroupState => ({
    ...g,
    entries: g.entries.filter((e) => e.id !== entryId),
    confirmed: false,
  });
  if (groupId === "new") {
    const created: ImportGroupState = {
      id: nextId("group"),
      createdAt: source.createdAt,
      entries: [entry],
      confirmed: false,
    };
    return [...state.map((g) => (g.id === source.id ? without(g) : g)), created].filter((g) => g.entries.length > 0);
  }
  if (!state.some((g) => g.id === groupId)) return state;
  return state
    .map((g) => {
      if (g.id === source.id) return without(g);
      if (g.id === groupId) return { ...g, entries: [...g.entries, entry], confirmed: false };
      return g;
    })
    .filter((g) => g.entries.length > 0);
}

export function editEntry(
  state: ImportState,
  entryId: string,
  patch: Partial<Pick<ImportEntry, "chip" | "axis" | "point">>,
): ImportState {
  return state.map((g) => ({
    ...g,
    entries: g.entries.map((e) =>
      e.id === entryId ? { ...e, ...patch, chipGuessed: patch.chip === undefined ? e.chipGuessed : false } : e,
    ),
  }));
}

export function removeEntry(state: ImportState, entryId: string): ImportState {
  return state
    .map((g) =>
      g.entries.some((e) => e.id === entryId)
        ? { ...g, entries: g.entries.filter((e) => e.id !== entryId), confirmed: false }
        : g,
    )
    .filter((g) => g.entries.length > 0);
}

export function setCreatedAt(state: ImportState, groupId: string, date: Date): ImportState {
  return state.map((g) => (g.id === groupId ? { ...g, createdAt: date } : g));
}

export function confirmGroup(state: ImportState, groupId: string, value: boolean): ImportState {
  return state.map((g) => (g.id === groupId ? { ...g, confirmed: value } : g));
}

export function groupBytes(group: ImportGroupState): number {
  return group.entries.reduce((sum, e) => sum + e.file.size, 0);
}

export function problems(group: ImportGroupState): ImportProblem[] {
  const found: ImportProblem[] = [];
  const seen = new Set<string>();
  for (const e of group.entries) {
    if (e.chip.trim() === "") found.push({ entryId: e.id, text: "Choose a chip." });
    if (e.axis === "") found.push({ entryId: e.id, text: "Choose an axis." });
    const point = parsePointText(e.point);
    if (point === "invalid") {
      found.push({ entryId: e.id, text: "The point must be three numbers: x, y, z." });
    }
    if (e.chip.trim() !== "" && e.axis !== "" && point !== "invalid") {
      const key = JSON.stringify([e.chip.trim(), e.axis, point]);
      if (seen.has(key)) {
        found.push({ entryId: e.id, text: "Another file has the same chip, axis and point." });
      }
      seen.add(key);
    }
  }
  if (Number.isNaN(group.createdAt.getTime())) {
    found.push({ entryId: null, text: "Pick a valid date and time." });
  }
  if (group.entries.length > MAX_IMPORT_FILES) {
    found.push({
      entryId: null,
      text: `A session takes at most ${MAX_IMPORT_FILES} files. This group has ${group.entries.length}.`,
    });
  }
  const bytes = groupBytes(group);
  if (bytes > MAX_IMPORT_BYTES) {
    found.push({
      entryId: null,
      text: `The files total ${formatBytes(bytes)}. The limit is ${formatBytes(MAX_IMPORT_BYTES)}.`,
    });
  }
  return found;
}

export function needsConfirmation(group: ImportGroupState): boolean {
  if (group.entries.length < 2) return false;
  const first = group.entries[0].stampToken;
  return first === null || group.entries.some((e) => e.stampToken !== first);
}

export function canImport(group: ImportGroupState): boolean {
  return problems(group).length === 0 && (!needsConfirmation(group) || group.confirmed);
}
