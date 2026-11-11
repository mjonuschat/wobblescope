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
  start?: Reply | ((call: ApiCall) => Reply);
}

function api({ settings = printerSettings(), chips = chipsSnapshot(), checked, start }: Options = {}) {
  return stubApiWithStatus((call) => {
    const path = apiPath(call);
    if (path === "settings") return { body: settings };
    if (path === "chips") return { body: chips };
    if (path === "chips/check") return { body: checked ?? chips };
    if (path === "tests" && call.method === "POST") {
      const reply = typeof start === "function" ? start(call) : start;
      return reply ?? { status: 202, body: { id: "test-1" } };
    }
    return { status: 404, body: { error: "not_found", message: "no" } };
  });
}

const startButton = () => screen.getByRole("button", { name: /Start test|Checking printer/ });
const pressed = (name: string) => screen.getByRole("button", { name }).getAttribute("aria-pressed") === "true";
const field = (label: RegExp | string) => screen.getByLabelText(label) as HTMLInputElement;
const posted = (calls: ApiCall[]) => calls.filter((c) => c.method === "POST" && apiPath(c) === "tests");

async function open(options?: Options) {
  const calls = api(options);
  renderRoute("/run/shaper");
  await screen.findByRole("button", { name: "Start test" });
  return calls;
}

describe("the run screen", () => {
  beforeEach(() => stubWebSocket());

  it("shows the printer values as placeholders, preselects the mode and enables Start", async () => {
    await open();
    expect(field(/End frequency/).placeholder).toBe("135");
    expect(field(/Acceleration per Hz/).placeholder).toBe("60");
    expect(pressed("Sweep")).toBe(true);
    expect(pressed("lis2dw ebb36")).toBe(true);
    expect(pressed("adxl345")).toBe(false);
    expect((startButton() as HTMLButtonElement).disabled).toBe(false);
  });

  it("preselects pulse when the printer sweeps with a period of 0", async () => {
    await open({ settings: { ...printerSettings(), default_mode: "Pulse" } });
    expect(pressed("Pulse")).toBe(true);
  });

  it("shows the emergency stop consequence before a test starts", async () => {
    await open();
    expect(screen.getByText(/Emergency stop puts Klipper into shutdown/)).toBeTruthy();
    expect(screen.getByText(/FIRMWARE_RESTART/)).toBeTruthy();
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

  it("shows two points and two axes as 4 segments and changes the time when hz_per_sec is overridden", async () => {
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Add a point" }));
    // 130 s of sweep plus 15 s of overhead per segment.
    expect(screen.getByText(/4 segments · about 9 min 40 s/)).toBeTruthy();
    fireEvent.change(field(/Hz per second/), { target: { value: "2" } });
    // 65 s plus 15 s per segment.
    expect(screen.getByText(/4 segments · about 5 min 20 s/)).toBeTruthy();
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
        const body = path === "status" ? healthyStatus : path === "settings" ? printerSettings() : chipsSnapshot();
        return new Response(JSON.stringify(body), { status: 200 });
      }),
    );
    renderRoute("/run/shaper");
    fireEvent.click(await screen.findByRole("button", { name: "Start test" }));
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

  it("hides the sweeping fields in pulse mode", async () => {
    await open();
    expect(screen.queryByLabelText(/Sweeping period/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Pulse" }));
    expect(screen.queryByLabelText(/Sweeping period/)).toBeNull();
    expect(screen.queryByLabelText(/Sweeping acceleration/)).toBeNull();
    expect(screen.getByLabelText(/End frequency/)).toBeTruthy();
  });

  it("lets chips be toggled and refuses to start with none", async () => {
    const calls = await open();
    fireEvent.click(screen.getByRole("button", { name: "adxl345" }));
    expect(pressed("adxl345")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "lis2dw ebb36" }));
    fireEvent.click(screen.getByRole("button", { name: "adxl345" }));
    expect(screen.getByText("Choose at least one chip.")).toBeTruthy();
    expect((startButton() as HTMLButtonElement).disabled).toBe(true);
    expect(posted(calls)).toHaveLength(0);
  });

  it("disables the toggle of an unavailable chip", async () => {
    await open({
      chips: chipsSnapshot({
        chips: [chip("adxl345"), chip("lis2dw ebb36", { availability: { state: "unavailable", reason: "gone" } })],
      }),
    });
    expect((screen.getByRole("button", { name: "lis2dw ebb36" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "adxl345" }) as HTMLButtonElement).disabled).toBe(false);
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
    expect(screen.queryByRole("button", { name: "Start test" })).toBeNull();

    ready = true;
    await client.invalidateQueries({ queryKey: queryKeys.status });
    expect(await screen.findByRole("button", { name: "Start test" })).toBeTruthy();
    expect(pressed("lis2dw ebb36")).toBe(true);
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
