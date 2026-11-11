import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";

// jsdom does not implement scrolling, which the router calls on navigation.
window.scrollTo = () => {};

// jsdom does not implement media queries, which the sidebar reads to tell a phone from a desktop.
window.matchMedia = (query: string) =>
  ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  }) as MediaQueryList;

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.head.querySelectorAll("base").forEach((b) => {
    b.remove();
  });
});
