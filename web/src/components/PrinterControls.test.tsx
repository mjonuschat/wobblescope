import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { ApiCall, Reply } from "@/test/http";
import { printerSettings } from "@/test/capture";
import { renderRoute } from "@/test/render";
import { queryKeys } from "@/lib/api/queries";
import { apiPath, healthyStatus, stubApiWithStatus } from "@/test/status";

import { STOP_NOTICE_MS, homedTone } from "./PrinterControls";

function api(homed = "xyz", extra: (path: string) => Reply | null = () => null) {
  return stubApiWithStatus((call) => {
    const path = apiPath(call);
    const custom = extra(path);
    if (custom) return custom;
    if (path === "printer") return { body: { homed_axes: homed, busy: null } };
    if (path === "printer/emergency-stop") return { body: { dispatched: true } };
    if (path === "printer/home") return { body: { homed: true } };
    if (path === "settings") return { body: printerSettings() };
    return { status: 404, body: { error: "not_found", message: "no" } };
  });
}

const posts = (calls: ApiCall[], path: string) => calls.filter((c) => c.method === "POST" && apiPath(c) === path);

describe("homedTone", () => {
  it("is full for X, Y and Z, partial for some and none for no axis", () => {
    expect(homedTone("xyz")).toBe("full");
    expect(homedTone("xy")).toBe("partial");
    expect(homedTone("")).toBe("none");
    expect(homedTone(null)).toBe("none");
  });
});

describe("the printer controls in the header", () => {
  it("has no home button in the header, only the emergency stop", async () => {
    api("xyz");
    renderRoute("/");
    await screen.findByRole("button", { name: "Stop printer" });
    expect(screen.queryByRole("button", { name: "Home" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Homing" })).toBeNull();
  });

  it("needs two presses to stop the printer", async () => {
    const calls = api();
    renderRoute("/");
    fireEvent.click(await screen.findByRole("button", { name: "Stop printer" }));
    expect(posts(calls, "printer/emergency-stop")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Press again to stop printer" }));
    await waitFor(() => expect(posts(calls, "printer/emergency-stop")).toHaveLength(1));
    expect(await screen.findByText("Stop sent")).toBeTruthy();
  });

  it("keeps Stop sent while Klipper is down and clears it when Klipper is ready again", async () => {
    let klippy: "ready" | "shutdown" = "ready";
    const status = () => ({
      body: {
        ...healthyStatus,
        klippy: { state: klippy, message: klippy === "ready" ? null : "Shutdown due to webhooks request" },
      },
    });
    // `api` fixes a healthy status, so this test stubs one it can change.
    stubApiWithStatus((call) => {
      const path = apiPath(call);
      if (path === "printer") return { body: { homed_axes: "xyz", busy: null } };
      if (path === "printer/emergency-stop") return { body: { dispatched: true } };
      return { status: 404, body: { error: "not_found", message: "no" } };
    }, status);
    const { client } = renderRoute("/");
    fireEvent.click(await screen.findByRole("button", { name: "Stop printer" }));
    fireEvent.click(screen.getByRole("button", { name: "Press again to stop printer" }));
    expect(await screen.findByText("Stop sent")).toBeTruthy();
    klippy = "shutdown";
    await client.invalidateQueries({ queryKey: queryKeys.status });
    await waitFor(() => expect(screen.getByText("Stop sent")).toBeTruthy());
    klippy = "ready";
    await client.invalidateQueries({ queryKey: queryKeys.status });
    await waitFor(() => expect(screen.queryByText("Stop sent")).toBeNull());
  });

  it("clears Stop sent after a while when Klipper was never seen going down", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"], shouldAdvanceTime: true });
    try {
      api();
      renderRoute("/");
      fireEvent.click(await screen.findByRole("button", { name: "Stop printer" }));
      fireEvent.click(screen.getByRole("button", { name: "Press again to stop printer" }));
      expect(await screen.findByText("Stop sent")).toBeTruthy();
      await act(async () => {
        vi.advanceTimersByTime(STOP_NOTICE_MS + 100);
      });
      expect(screen.queryByText("Stop sent")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });
});
