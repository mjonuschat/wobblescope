const SECTION = "[input_shaper]\n";

/**
 * The config of both axes in one block: the x snippet, then the y lines without a second
 * `[input_shaper]` line. `null` unless both axes have a snippet.
 */
export function pairSnippet(x: string | null, y: string | null): string | null {
  if (x === null || y === null) return null;
  return x + (y.startsWith(SECTION) ? y.slice(SECTION.length) : y);
}
