import { useSyncExternalStore } from "react";

export type ResultsLayout = "tabs" | "side-by-side";

export const RESULTS_LAYOUTS: readonly ResultsLayout[] = ["tabs", "side-by-side"];

const KEY = "wobblescope-results-layout";

export function parseLayout(value: unknown): ResultsLayout {
  return value === "side-by-side" ? value : "tabs";
}

const listeners = new Set<() => void>();
/** What a browser that refuses storage access (privacy mode) keeps until the page closes. */
let fallback: ResultsLayout = "tabs";

function read(): ResultsLayout {
  try {
    return parseLayout(window.localStorage.getItem(KEY));
  } catch {
    return fallback;
  }
}

export function setResultsLayout(layout: ResultsLayout): void {
  fallback = layout;
  try {
    window.localStorage.setItem(KEY, layout);
  } catch {
    // The fallback holds it.
  }
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  // Another tab changing the choice.
  const onStorage = (event: StorageEvent) => {
    if (event.key === KEY) listener();
  };
  window.addEventListener("storage", onStorage);
  return () => {
    listeners.delete(listener);
    window.removeEventListener("storage", onStorage);
  };
}

export function useResultsLayout(): ResultsLayout {
  return useSyncExternalStore(subscribe, read, () => "tabs");
}
