export type ThemeMode = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

export const THEME_MODES: readonly ThemeMode[] = ["system", "light", "dark"];

export function parseMode(value: unknown): ThemeMode {
  return value === "light" || value === "dark" ? value : "system";
}

export function resolveTheme(mode: ThemeMode, prefersDark: boolean): ResolvedTheme {
  if (mode === "system") return prefersDark ? "dark" : "light";
  return mode;
}

/** Reads the stored mode. A browser that refuses storage access (privacy mode) gives System. */
export function readStoredMode(key: string): ThemeMode {
  try {
    return parseMode(window.localStorage.getItem(key));
  } catch {
    return "system";
  }
}

export function storeMode(key: string, mode: ThemeMode): void {
  try {
    window.localStorage.setItem(key, mode);
  } catch {
    // The choice then lasts until the page closes.
  }
}

/** Sets the `dark` class and the native color scheme that form controls and scrollbars follow. */
export function applyTheme(theme: ResolvedTheme, root: HTMLElement = document.documentElement): void {
  root.classList.toggle("dark", theme === "dark");
  root.style.colorScheme = theme;
}
