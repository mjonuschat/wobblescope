import { describe, expect, it } from "vitest";

import { formatBytes, formatHz, formatTime } from "./format";

describe("formatBytes", () => {
  it.each([
    [0, "0 B"],
    [1536, "1.5 KiB"],
    [100 * 1024 * 1024, "100 MiB"],
    [1.5 * 1024 ** 3, "1.5 GiB"],
  ])("formats %d as %s", (bytes, text) => {
    expect(formatBytes(bytes)).toBe(text);
  });
});

describe("formatHz", () => {
  it("rounds to 0.1 Hz so float noise never shows", () => {
    expect(formatHz(52.60000000000019)).toBe("52.6");
    expect(formatHz(48.2)).toBe("48.2");
  });
});

describe("formatTime", () => {
  it("writes YYYY-MM-DD HH:MM:SS on a 24 hour clock", () => {
    const date = new Date(2026, 10, 5, 23, 8, 9);
    expect(formatTime(date.toISOString())).toBe("2026-11-05 23:08:09");
  });

  it("pads the single digits and keeps the afternoon in 24 hours", () => {
    const date = new Date(2026, 0, 2, 3, 4, 5);
    expect(formatTime(date.toISOString())).toBe("2026-01-02 03:04:05");
    const pm = new Date(2026, 0, 2, 18, 5, 0);
    expect(formatTime(pm.toISOString())).toBe("2026-01-02 18:05:00");
  });

  it("returns text that is not a time as it was", () => {
    expect(formatTime("later")).toBe("later");
  });
});
