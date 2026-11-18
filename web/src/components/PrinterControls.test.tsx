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

/** Home is disabled until the status says Klipper is ready. */
async function clickHome() {
  const button = (await screen.findByRole("button", { name: "Home" })) as HTMLButtonElement;
  await waitFor(() => expect(button.disabled).toBe(false));
  fireEvent.click(button);
}

describe("homedTone", () => {
  it("is full for X, Y and Z, partial for some and none for no axis", () => {
    expect(homedTone("xyz")).toBe("full");
    expect(homedTone("xy")).toBe("partial");
    expect(homedTone("")).toBe("none");
    expect(homedTone(null)).toBe("none");
  });
});

describe("the printer controls in the header", () => {
  it("colours the home button by how much is homed", async () => {
    api("xyz");
    renderRoute("/");
    const button = await screen.findByRole("button", { name: "Home" });
    await waitFor(() => expect(button.className).toContain("text-success"));
  });

  it("shows a partly homed printer in the warning color", async () => {
    api("xy");
    renderRoute("/");
    const button = await screen.findByRole("button", { name: "Home" });
    await waitFor(() => expect(button.className).toContain("text-warning"));
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

  it("homes the printer with one press", async () => {
    const calls = api();
    renderRoute("/");
    await clickHome();
    await waitFor(() => expect(posts(calls, "printer/home")).toHaveLength(1));
  });

  it("shows the daemon's message when homing is refused", async () => {
    api("", (path) =>
      path === "printer/home"
        ? { status: 409, body: { error: "preflight_failed", message: "a test is running", failures: [] } }
        : null,
    );
    renderRoute("/");
    await clickHome();
    expect(await screen.findByText("a test is running")).toBeTruthy();
  });
});
