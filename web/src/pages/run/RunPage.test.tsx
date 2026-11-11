import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ChipsSnapshot } from "@/lib/generated/ChipsSnapshot";
import type { PrinterSettings } from "@/lib/generated/PrinterSettings";
import { queryKeys } from "@/lib/api/queries";
import { chip, chipsSnapshot, printerSettings } from "@/test/capture";
import type { ApiCall, Reply } from "@/test/http";
import { renderRoute } from "@/test/render";
import { apiPath, healthyStatus, stubApiWithStatus } from "@/test/status";
import { stubWebSocket } from "@/test/websocket";

interface Options {
  settings?: PrinterSettings;
  chips?: ChipsSnapshot;
  checked?: ChipsSnapshot;
  homed?: string | null;
  start?: Reply | ((call: ApiCall) => Reply);
}

function api({ settings = printerSettings(), chips = chipsSnapshot(), checked, start, homed = "xyz" }: Options = {}) {
  return stubApiWithStatus((call) => {
    const path = apiPath(call);
    if (path === "settings") return { body: settings };
    if (path === "chips") return { body: chips };
    if (path === "chips/check") return { body: checked ?? chips };
    if (path === "printer") return { body: { homed_axes: homed } };
    if (path === "tests" && call.method === "POST") {
      const reply = typeof start === "function" ? start(call) : start;
      return reply ?? { status: 202, body: { id: "test-1" } };
    }
    return { status: 404, body: { error: "not_found", message: "no" } };
  });
}

const startButton = () => screen.getByRole("button", { name: /Run Test|Checking printer/ }) as HTMLButtonElement;
const thumb = (index: number) => document.querySelectorAll<HTMLInputElement>('input[type="range"]')[index];
const mode = () => screen.getByRole("combobox", { name: "Mode" }).textContent;
async function chooseMode(name: "Pulse" | "Sweep") {
  fireEvent.click(screen.getByRole("combobox", { name: "Mode" }));
  const option = await screen.findByRole("option", { name });
  fireEvent.pointerDown(option);
  fireEvent.pointerUp(option);
  fireEvent.click(option);
}
const isOn = (name: string) => screen.getByRole("switch", { name }).getAttribute("aria-checked") === "true";
const field = (label: RegExp | string) => screen.getByLabelText(label) as HTMLInputElement;
const posted = (calls: ApiCall[]) => calls.filter((c) => c.method === "POST" && apiPath(c) === "tests");

async function open(options?: Options) {
  const calls = api(options);
  renderRoute("/run/shaper");
  await screen.findByRole("button", { name: "Run Test" });
  await waitFor(() => expect(screen.queryByText(/Checking whether the printer is homed/)).toBeNull());
  return calls;
}

describe("the run screen", () => {
  beforeEach(() => stubWebSocket({ idle: true }));

  it("disables Start and says why while the printer is not fully homed", async () => {
    api({ homed: "xy" });
    renderRoute("/run/shaper");
    expect(await screen.findByText("Home the printer first. Not homed: Z.")).toBeTruthy();
    expect(startButton().disabled).toBe(true);
  });

  it("names every axis when none is homed", async () => {
    api({ homed: null });
    renderRoute("/run/shaper");
    expect(await screen.findByText("Home the printer first. Not homed: X, Y, Z.")).toBeTruthy();
  });

  it("fills the fields with the printer values, preselects the mode and enables Start", async () => {
    await open();
    expect(field(/End frequency/).value).toBe("135");
    expect(field(/Acceleration per Hz/).value).toBe("60");
    expect(mode()).toContain("Sweep");
    expect(isOn("lis2dw ebb36")).toBe(true);
    expect(isOn("adxl345")).toBe(false);
    expect((startButton() as HTMLButtonElement).disabled).toBe(false);
  });

  it("preselects pulse when the printer sweeps with a period of 0", async () => {
    await open({ settings: { ...printerSettings(), default_mode: "Pulse" } });
    expect(mode()).toContain("Pulse");
  });

  it("marks an unreadable freq_end as required and keeps Start disabled until it has a value", async () => {
    await open({
      settings: printerSettings({ freq_end: { value: null, overridable: true, needed: true } }),
    });
    const input = field(/End frequency/);
    expect(input.placeholder).toBe("required");
    expect(input.getAttribute("aria-required")).toBe("true");
    expect((startButton() as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(input, { target: { value: "100" } });
    expect((startButton() as HTMLButtonElement).disabled).toBe(false);
  });

  it("posts only the overridden fields with the chosen chips and points", async () => {
    const calls = await open();
    fireEvent.change(field(/End frequency/), { target: { value: "100" } });
    fireEvent.click(screen.getByRole("switch", { name: "Y" }));
    fireEvent.click(startButton());
    await waitFor(() => expect(posted(calls)).toHaveLength(1));
    expect(JSON.parse(String(posted(calls)[0].body))).toEqual({
      chips: ["lis2dw ebb36"],
      axes: ["X"],
      mode: "Sweep",
      overrides: { freq_end: 100 },
      points: [[150.5, 154, 20]],
    });
  });

  it("sends no override for a value equal to the printer's or for a cleared field", async () => {
    const calls = await open();
    fireEvent.change(field(/End frequency/), { target: { value: "135" } });
    fireEvent.change(field(/Hz per second/), { target: { value: "" } });
    fireEvent.click(startButton());
    await waitFor(() => expect(posted(calls)).toHaveLength(1));
    expect(JSON.parse(String(posted(calls)[0].body)).overrides).toEqual({});
  });

  it("shows a daemon message beside the field it names", async () => {
    await open({
      start: {
        status: 400,
        body: { error: "invalid_request", message: "hz_per_sec: must be above 0 and at most 2" },
      },
    });
    fireEvent.click(startButton());
    const alert = await screen.findByText("must be above 0 and at most 2");
    expect(alert.id).toBe("error-hz_per_sec");
    expect(field(/Hz per second/).getAttribute("aria-invalid")).toBe("true");
  });

  it("lists every failed pre-flight check", async () => {
    await open({
      start: {
        status: 409,
        body: {
          error: "preflight_failed",
          message: "2 checks failed",
          failures: [
            { check: "homed", message: "home X, Y and Z first; not homed: Z" },
            { check: "toolhead_idle", message: "the toolhead is Printing; wait until it is idle" },
          ],
        },
      },
    });
    fireEvent.click(startButton());
    expect(await screen.findByText("home X, Y and Z first; not homed: Z")).toBeTruthy();
    expect(screen.getByText("the toolhead is Printing; wait until it is idle")).toBeTruthy();
  });

  it("disables Start while the request is pending and sends nothing on a second click", async () => {
    let release: (response: Response) => void = () => {};
    const pending = new Promise<Response>((resolve) => {
      release = resolve;
    });
    let posts = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const path = new URL(String(input)).pathname.replace(/^.*\/api\//, "");
        if (path === "tests") {
          posts += 1;
          return pending;
        }
        const body =
          path === "status"
            ? healthyStatus
            : path === "settings"
              ? printerSettings()
              : path === "printer"
                ? { homed_axes: "xyz" }
                : chipsSnapshot();
        return new Response(JSON.stringify(body), { status: 200 });
      }),
    );
    renderRoute("/run/shaper");
    const run = (await screen.findByRole("button", { name: "Run Test" })) as HTMLButtonElement;
    await waitFor(() => expect(run.disabled).toBe(false));
    fireEvent.click(run);
    const busy = await screen.findByRole("button", { name: "Checking printer…" });
    expect((busy as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(busy);
    fireEvent.submit(busy.closest("form")!);
    expect(posts).toBe(1);
    release(new Response(JSON.stringify({ id: "test-1" }), { status: 202 }));
    // The 202 hands over to the starting view.
    expect(await screen.findByText("Starting…")).toBeTruthy();
    expect(posts).toBe(1);
  });

  it("turns an axis on and off with a switch and refuses to start with none", async () => {
    await open();
    const x = screen.getByRole("switch", { name: "X" });
    const y = screen.getByRole("switch", { name: "Y" });
    expect(x.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(x);
    expect(x.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(y);
    expect(screen.getByText("Choose at least one axis.")).toBeTruthy();
    expect((startButton() as HTMLButtonElement).disabled).toBe(true);
  });

  it("starts the range slider at the printer's range and follows the typed values", async () => {
    await open();
    const start = () => thumb(0);
    const end = () => thumb(1);
    expect([start().value, end().value]).toEqual(["5", "135"]);
    fireEvent.change(field(/End frequency/), { target: { value: "100" } });
    expect(end().value).toBe("100");
    expect(start().value).toBe("5");
  });

  it("moving a thumb overrides only that end of the range", async () => {
    const calls = await open();
    fireEvent.change(thumb(1), { target: { value: "90" } });
    expect(field(/End frequency/).value).toBe("90");
    expect(field(/Start frequency/).value).toBe("5");
    fireEvent.click(startButton());
    await waitFor(() => expect(posted(calls)).toHaveLength(1));
    expect(JSON.parse(String(posted(calls)[0].body)).overrides).toEqual({ freq_end: 90 });
  });

  it("adds a point after any row with its plus and removes one with the minus", async () => {
    await open();
    const xs = () =>
      [...document.querySelectorAll<HTMLInputElement>('input[id^="point-"][id$="-x"]')].map((i) => i.value);
    const hidden = (el: Element) => el.className.includes("invisible");
    expect(hidden(screen.getByRole("button", { name: "Remove point 1" }))).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Add a point after point 1" }));
    fireEvent.change(document.getElementById("point-1-x") as HTMLInputElement, { target: { value: "200" } });
    expect(xs()).toEqual(["150.5", "200"]);
    expect(hidden(screen.getByRole("button", { name: "Remove point 1" }))).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Add a point after point 1" }));
    expect(xs()).toEqual(["150.5", "150.5", "200"]);
    fireEvent.click(screen.getByRole("button", { name: "Remove point 2" }));
    expect(xs()).toEqual(["150.5", "200"]);
  });

  it("fills the points from the pattern menu on the first row", async () => {
    await open();
    const xs = () =>
      [...document.querySelectorAll<HTMLInputElement>('input[id^="point-"][id$="-x"]')].map((i) => i.value);
    expect(screen.getAllByRole("button", { name: /Pattern/ })).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: /Pattern/ }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "3 x 3 Grid" }));
    await waitFor(() => expect(xs()).toHaveLength(9));
    expect(xs().slice(0, 3)).toEqual(["20", "150.5", "281"]);
    fireEvent.click(screen.getByRole("button", { name: /Pattern/ }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Center + Corners" }));
    await waitFor(() => expect(xs()).toHaveLength(5));
  });

  it("hides the sweeping fields in pulse mode", async () => {
    await open();
    expect(screen.queryByLabelText(/Sweeping period/)).toBeTruthy();
    await chooseMode("Pulse");
    await waitFor(() => expect(screen.queryByLabelText(/Sweeping period/)).toBeNull());
    expect(screen.queryByLabelText(/Sweeping acceleration/)).toBeNull();
    expect(screen.getByLabelText(/End frequency/)).toBeTruthy();
  });

  it("lets chips be toggled and refuses to start with none", async () => {
    const calls = await open();
    fireEvent.click(screen.getByRole("switch", { name: "adxl345" }));
    expect(isOn("adxl345")).toBe(true);
    fireEvent.click(screen.getByRole("switch", { name: "lis2dw ebb36" }));
    fireEvent.click(screen.getByRole("switch", { name: "adxl345" }));
    expect(screen.getByText("Choose at least one chip.")).toBeTruthy();
    expect((startButton() as HTMLButtonElement).disabled).toBe(true);
    expect(posted(calls)).toHaveLength(0);
  });

  it("disables the switch of an unavailable chip", async () => {
    await open({
      chips: chipsSnapshot({
        chips: [chip("adxl345"), chip("lis2dw ebb36", { availability: { state: "unavailable", reason: "gone" } })],
      }),
    });
    expect(screen.getByRole("switch", { name: "lis2dw ebb36" }).getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByRole("switch", { name: "adxl345" }).getAttribute("aria-disabled")).not.toBe("true");
  });

  it("shows the form once the printer becomes ready after the page opened during startup", async () => {
    let ready = false;
    const starting = {
      moonraker: { state: "connecting" as const, message: null, version: null },
      klippy: { state: "unknown" as const, message: null },
      storage: { state: "waiting" as const },
    };
    stubApiWithStatus(
      (call) => {
        const path = apiPath(call);
        if (path === "settings") {
          return ready
            ? { body: printerSettings() }
            : {
                status: 503,
                body: { error: "moonraker_unavailable", message: "the daemon is not connected to Moonraker" },
              };
        }
        if (path === "chips") {
          return { body: ready ? chipsSnapshot() : chipsSnapshot({ known: false, current: false, chips: [] }) };
        }
        return { status: 404, body: { error: "not_found", message: "no" } };
      },
      () => ({ body: ready ? healthyStatus : starting }),
    );
    const { client } = renderRoute("/run/shaper");
    expect(await screen.findByText("the daemon is not connected to Moonraker")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Run Test" })).toBeNull();

    ready = true;
    await client.invalidateQueries({ queryKey: queryKeys.status });
    expect(await screen.findByRole("button", { name: "Run Test" })).toBeTruthy();
    expect(isOn("lis2dw ebb36")).toBe(true);
  });
});

describe("the run test routes", () => {
  it("sends /run to the input shaper page", async () => {
    api();
    const { router } = renderRoute("/run");
    await screen.findByRole("heading", { name: "Input Shaper" });
    expect(router.state.location.pathname).toBe("/run/shaper");
  });
});
