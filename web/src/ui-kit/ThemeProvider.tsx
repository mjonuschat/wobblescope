import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

import {
  applyTheme,
  readStoredMode,
  resolveTheme,
  storeMode,
  type ResolvedTheme,
  type ThemeMode,
} from "@/ui-kit/theme";

const DARK_QUERY = "(prefers-color-scheme: dark)";

interface ThemeState {
  mode: ThemeMode;
  resolved: ResolvedTheme;
  setMode: (mode: ThemeMode) => void;
}

const ThemeContext = createContext<ThemeState>({ mode: "system", resolved: "dark", setMode: () => {} });

function systemPrefersDark(): boolean {
  return window.matchMedia(DARK_QUERY).matches;
}

interface Props {
  /** The `localStorage` key. It must match the key in the inline script of `index.html`. */
  storageKey: string;
  children: ReactNode;
}

export function ThemeProvider({ storageKey, children }: Props) {
  const [mode, setModeState] = useState<ThemeMode>(() => readStoredMode(storageKey));
  const [prefersDark, setPrefersDark] = useState(systemPrefersDark);

  useEffect(() => {
    const query = window.matchMedia(DARK_QUERY);
    const onChange = () => setPrefersDark(query.matches);
    onChange();
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  const resolved = resolveTheme(mode, prefersDark);
  useEffect(() => applyTheme(resolved), [resolved]);

  const setMode = useCallback(
    (next: ThemeMode) => {
      setModeState(next);
      storeMode(storageKey, next);
    },
    [storageKey],
  );

  const value = useMemo(() => ({ mode, resolved, setMode }), [mode, resolved, setMode]);
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

/** The chosen mode and the theme it resolves to. Outside a provider it reads as System on dark. */
export function useTheme(): ThemeState {
  return useContext(ThemeContext);
}
