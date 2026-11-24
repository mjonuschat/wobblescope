import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { printerSettings } from "@/test/capture";
import { renderRoute } from "@/test/render";
import { apiPath, healthyStatus, stubApiWithStatus } from "@/test/status";

import { activityConnection, homedConnection } from "./StatusDots";

describe("activityConnection", () => {
  it("shows nothing while Klipper is not ready", () => {
    expect(activityConnection(false, "A test is running.", null)).toBeNull();
  });

  it("is idle when nothing runs", () => {
    expect(activityConnection(true, null, null)).toMatchObject({ label: "Idle", tone: "off" });
  });

  it.each([
    ["The printer is homing.", "Homing"],
    ["The printer is levelling.", "Levelling"],
    ["The chips are being checked.", "Checking chips"],
    ["A hold is running.", "Hold running"],
    ["A test is running.", "Test running"],
    ["The toolhead is moving.", "Moving"],
    ["The printer may still be moving.", "Moving"],
  ])("puts %s as %s, with the sentence in the tooltip", (busy, word) => {
    expect(activityConnection(true, busy, null)).toEqual({ label: word, tone: "warn", details: [busy] });
  });

  it("names an unknown busy reason Busy", () => {
    expect(activityConnection(true, "Something else.", null)?.label).toBe("Busy");
  });

  it("counts a move started in this browser before the poll sees it", () => {
    expect(activityConnection(true, null, "Preparing")).toMatchObject({ label: "Preparing", tone: "warn" });
  });
});

describe("homedConnection", () => {
  it("is ok when X, Y and Z are homed", () => {
    expect(homedConnection(true, "xyz")).toMatchObject({ label: "Homed", tone: "ok" });
  });

  it("is a warning when only some axes are homed, and lists them", () => {
    const item = homedConnection(true, "xy");
    expect(item.tone).toBe("warn");
    expect(item.details).toEqual(["Homed axes: X, Y"]);
  });

  it("is off when no axis is homed", () => {
    expect(homedConnection(true, "")).toMatchObject({ tone: "off", details: ["No axes homed"] });
    expect(homedConnection(true, null).tone).toBe("off");
  });

  it("is off, not an error, when Klipper is not ready", () => {
    expect(homedConnection(false, "xyz")).toMatchObject({ tone: "off", details: ["Klipper is not ready"] });
  });

  it("is never an error tone", () => {
    for (const ready of [true, false]) {
      for (const axes of [null, "", "x", "xy", "xyz"]) {
        expect(homedConnection(ready, axes).tone).not.toBe("error");
      }
    }
  });
});

function api(homed: string, status = healthyStatus) {
  stubApiWithStatus((call) => {
    const path = apiPath(call);
    if (path === "printer")
      return { body: { homed_axes: homed, busy: null, levelled: null, level_kind: null, level_command: null } };
    if (path === "settings") return { body: printerSettings() };
    return { status: 404, body: { error: "not_found", message: "no" } };
  }, status);
}

async function homedDot() {
  const list = await screen.findByRole("list", { name: "Connections" });
  const item = within(list).getByText("Homed").closest("li") as HTMLElement;
  return item.querySelector("[aria-hidden='true']") as HTMLElement;
}

describe("the Homed dot in the header", () => {
  it("comes after Storage and is green when X, Y and Z are homed", async () => {
    api("xyz");
    renderRoute("/");
    const list = await screen.findByRole("list", { name: "Connections" });
    const labels = within(list)
      .getAllByRole("listitem")
      .map((li) => li.textContent);
    expect(labels[3]).toContain("Homed");
    expect(labels[2]).toContain("Storage");
    await waitFor(async () => expect((await homedDot()).className).toContain("bg-success"));
  });

  it("is amber when some axes are homed", async () => {
    api("xy");
    renderRoute("/");
    await waitFor(async () => expect((await homedDot()).className).toContain("bg-warning"));
  });

  it("is grey when no axis is homed", async () => {
    api("");
    renderRoute("/");
    await screen.findByText("Homed");
    const dot = await homedDot();
    await waitFor(() => expect(dot.className).toContain("bg-muted-foreground"));
    expect(dot.className).not.toContain("bg-destructive");
  });

  it("is grey when Klipper is not ready", async () => {
    api("xyz", { ...healthyStatus, klippy: { state: "shutdown", message: "x" } });
    renderRoute("/");
    await screen.findByText("Homed");
    const dot = await homedDot();
    expect(dot.className).toContain("bg-muted-foreground");
  });
});
