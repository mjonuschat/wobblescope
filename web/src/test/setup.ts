import { cleanup } from "@testing-library/react";
import { toast } from "sonner";
import { afterAll, afterEach, vi } from "vitest";

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

// Sonner removes a closed toast 200 ms later, with a timer it does not clear when the Toaster
// unmounts. If the test file ends first, jsdom is gone when that timer fires, and Vitest fails
// the whole run on the resulting "window is not defined". A file that showed a toast waits the
// timer out.
afterAll(async () => {
  if (toast.getHistory().length > 0) {
    await new Promise((resolve) => setTimeout(resolve, 300));
  }
});
