import { afterEach, describe, expect, it, vi } from "vitest";

import { applyTheme, parseMode, readStoredMode, resolveTheme, storeMode } from "@/ui-kit/theme";

afterEach(() => {
  window.localStorage.clear();
  vi.restoreAllMocks();
});

describe("parseMode", () => {
  it("accepts the three modes and reads anything else as system", () => {
    expect(parseMode("light")).toBe("light");
    expect(parseMode("dark")).toBe("dark");
    expect(parseMode("system")).toBe("system");
    expect(parseMode("sepia")).toBe("system");
    expect(parseMode(null)).toBe("system");
  });
});

describe("resolveTheme", () => {
  it("follows the system preference only in system mode", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });
});

describe("stored mode", () => {
  it("round-trips through localStorage", () => {
    storeMode("k", "light");
    expect(readStoredMode("k")).toBe("light");
  });

  it("reads system when nothing is stored", () => {
    expect(readStoredMode("k")).toBe("system");
  });

  it("survives storage that throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    expect(readStoredMode("k")).toBe("system");
    expect(() => storeMode("k", "dark")).not.toThrow();
  });
});

describe("applyTheme", () => {
  it("sets the dark class and the color scheme", () => {
    const root = document.createElement("html");
    applyTheme("dark", root);
    expect(root.classList.contains("dark")).toBe(true);
    expect(root.style.colorScheme).toBe("dark");
    applyTheme("light", root);
    expect(root.classList.contains("dark")).toBe(false);
    expect(root.style.colorScheme).toBe("light");
  });
});
