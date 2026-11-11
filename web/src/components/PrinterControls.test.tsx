import { fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { ApiCall, Reply } from "@/test/http";
import { printerSettings } from "@/test/capture";
import { renderRoute } from "@/test/render";
import { apiPath, stubApiWithStatus } from "@/test/status";

import { homedTone } from "./PrinterControls";

function api(homed = "xyz", extra: (path: string) => Reply | null = () => null) {
  return stubApiWithStatus((call) => {
    const path = apiPath(call);
    const custom = extra(path);
    if (custom) return custom;
    if (path === "printer") return { body: { homed_axes: homed } };
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
    await waitFor(() => expect(button.className).toContain("text-emerald-400"));
  });

  it("shows a partly homed printer in amber", async () => {
    api("xy");
    renderRoute("/");
    const button = await screen.findByRole("button", { name: "Home" });
    await waitFor(() => expect(button.className).toContain("text-amber-400"));
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
