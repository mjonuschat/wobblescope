import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { printerSettings } from "@/test/capture";
import type { Reply } from "@/test/http";
import { renderRoute } from "@/test/render";
import { apiPath, healthyStatus, stubApiWithStatus } from "@/test/status";

interface Printer {
  homed_axes: string | null;
  busy: string | null;
  levelled: boolean | null;
  level_kind: "z_tilt" | "quad_gantry_level" | null;
  level_command: string | null;
}

const base: Printer = { homed_axes: "xyz", busy: null, levelled: null, level_kind: null, level_command: null };

function api(printer: Partial<Printer> = {}, extra: (path: string) => Reply | null = () => null) {
  return stubApiWithStatus((call) => {
    const path = apiPath(call);
    const custom = extra(path);
    if (custom) return custom;
    if (path === "printer") return { body: { ...base, ...printer } };
    if (path === "printer/home") return { body: { homed: true } };
    if (path === "printer/level") return { body: { levelled: true } };
    if (path === "settings") return { body: printerSettings() };
    return { status: 404, body: { error: "not_found", message: "no" } };
  });
}

async function button(name: string) {
  return (await screen.findByRole("button", { name })) as HTMLButtonElement;
}

async function enabled(name: string) {
  const b = await button(name);
  await waitFor(() => expect(b.disabled).toBe(false));
  return b;
}

const posts = (calls: { method: string; url: URL }[], path: string) =>
  calls.filter((c) => c.method === "POST" && apiPath(c as never) === path);

describe("the printer card", () => {
  it("shows which axes are homed", async () => {
    api({ homed_axes: "xy" });
    renderRoute("/");
    const list = await screen.findByRole("list", { name: "Printer status" });
    await waitFor(() => expect(within(list).getByText("X homed")).toBeTruthy());
    expect(within(list).getByText("Z not homed")).toBeTruthy();
    expect(within(list).getByText("Y homed")).toBeTruthy();
  });

  it("posts the axes each home button names", async () => {
    const calls = api();
    renderRoute("/");
    const expected: [string, string][] = [
      ["Home XY", "xy"],
      ["Home X", "x"],
      ["Home Y", "y"],
      ["Home Z", "z"],
      ["Home all", "all"],
    ];
    for (const [name, axes] of expected) {
      fireEvent.click(await enabled(name));
      await waitFor(() =>
        expect(posts(calls, "printer/home")).toHaveLength(expected.findIndex((e) => e[1] === axes) + 1),
      );
      const sent = posts(calls, "printer/home").at(-1) as unknown as { body: string };
      expect(JSON.parse(sent.body)).toEqual({ axes });
      await enabled(name);
    }
  });

  it("has no level button or indicator on a printer without levelling", async () => {
    api();
    renderRoute("/");
    await enabled("Home all");
    expect(screen.queryByRole("button", { name: /Z-tilt adjust|Quad gantry level/ })).toBeNull();
    expect(screen.queryByText("Levelled")).toBeNull();
    expect(screen.queryByText("Not levelled")).toBeNull();
  });

  it("shows the level button with the command in its tooltip, and the levelled state", async () => {
    api({ level_kind: "quad_gantry_level", levelled: true, level_command: "QUAD_GANTRY_LEVEL" });
    renderRoute("/");
    const level = await enabled("Quad gantry level");
    expect(level.textContent).toBe("QGL");
    expect(level.title).toBe("Runs QUAD_GANTRY_LEVEL");
    expect(await screen.findByText("Levelled")).toBeTruthy();
  });

  it("says not levelled and names Z_TILT_ADJUST for a z_tilt printer", async () => {
    api({ level_kind: "z_tilt", levelled: false, level_command: "Z_TILT_ADJUST" });
    renderRoute("/");
    const level = await enabled("Z-tilt adjust");
    expect(level.title).toBe("Runs Z_TILT_ADJUST");
    expect(level.textContent).toBe("Z-TILT");
    expect(await screen.findByText("Not levelled")).toBeTruthy();
  });

  it("names the configured command in the tooltip when the daemon reports one", async () => {
    api({ level_kind: "z_tilt", levelled: false, level_command: "LEVEL_BED" });
    renderRoute("/");
    expect((await enabled("Z-tilt adjust")).title).toBe("Runs LEVEL_BED");
  });

  it("posts the level request and says it cannot be cancelled while it runs", async () => {
    const calls = api({ level_kind: "z_tilt", levelled: false });
    let release: () => void = () => {};
    const held = new Promise<void>((resolve) => (release = resolve));
    const inner = globalThis.fetch;
    vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
      if (String(input).endsWith("/printer/level")) await held;
      return inner(input, init);
    });
    renderRoute("/");
    fireEvent.click(await enabled("Z-tilt adjust"));
    expect(await screen.findByText(/cannot be cancelled except with the emergency stop/)).toBeTruthy();
    expect((await button("Home all")).disabled).toBe(true);
    release();
    await waitFor(() => expect(posts(calls, "printer/level")).toHaveLength(1));
    await waitFor(() => expect(screen.queryByText(/cannot be cancelled/)).toBeNull());
  });

  it("disables every button while a test runs and says why", async () => {
    api({ busy: "A test is running.", level_kind: "z_tilt", levelled: false });
    renderRoute("/");
    expect(await screen.findByText("A test is running.")).toBeTruthy();
    for (const name of ["Home XY", "Home X", "Home Y", "Home Z", "Home all", "Z-tilt adjust"]) {
      expect((await button(name)).disabled).toBe(true);
    }
  });

  it("disables the buttons while Klipper is not ready", async () => {
    stubApiWithStatus(
      (call) => (apiPath(call) === "settings" ? { body: printerSettings() } : { status: 404, body: {} }),
      { ...healthyStatus, klippy: { state: "shutdown", message: "x" } },
    );
    renderRoute("/");
    expect(await screen.findByText("Klipper is not ready.")).toBeTruthy();
    expect((await button("Home all")).disabled).toBe(true);
  });

  it("shows an error from the daemon on the card", async () => {
    api({}, (path) =>
      path === "printer/home"
        ? { status: 503, body: { error: "moonraker_unavailable", message: "homing failed: timeout" } }
        : null,
    );
    renderRoute("/");
    fireEvent.click(await enabled("Home Z"));
    expect(await screen.findByText("homing failed: timeout")).toBeTruthy();
  });
});
