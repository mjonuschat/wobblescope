import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { chip, printerSettings } from "@/test/capture";
import { renderRoute } from "@/test/render";
import { apiPath, stubApiWithStatus } from "@/test/status";

import { measure } from "./PrinterPage";

function api() {
  stubApiWithStatus((call) => {
    switch (apiPath(call)) {
      case "settings":
        return { body: printerSettings() };
      case "chips":
        return {
          body: {
            known: true,
            current: true,
            chips: [chip("adxl345"), chip("lis2dw ebb36")],
            skipped: [],
            defaults: { x: ["lis2dw ebb36"], y: ["lis2dw ebb36"] },
          },
        };
      default:
        return { status: 404, body: { error: "not_found", message: "no" } };
    }
  });
}

describe("the printer page", () => {
  it("is the landing page and shows the machine, its limits and travel", async () => {
    api();
    renderRoute("/");
    expect(await screen.findByRole("heading", { name: "Printer" })).toBeTruthy();
    expect(await screen.findByText("corexy")).toBeTruthy();
    expect(screen.getByText("500 mm/s")).toBeTruthy();
    expect(screen.getByText("20000 mm/s²")).toBeTruthy();
    expect(screen.getByText("0 to 301 mm (301 mm)")).toBeTruthy();
  });

  it("says unknown for a limit Klipper did not report", async () => {
    api();
    renderRoute("/");
    const label = await screen.findByText("Max Z acceleration");
    expect(within(label.parentElement as HTMLElement).getByText("unknown")).toBeTruthy();
  });

  it("names the axes each chip is the default for", async () => {
    api();
    renderRoute("/");
    const row = (await screen.findByText("lis2dw ebb36")).closest("tr") as HTMLElement;
    expect(within(row).getByText("X, Y")).toBeTruthy();
    const other = screen.getAllByText("adxl345")[0].closest("tr") as HTMLElement;
    expect(within(other).getByText("none")).toBeTruthy();
  });

  it("shows the sweeping settings only in sweep mode", async () => {
    api();
    renderRoute("/");
    expect(await screen.findByText("Sweeping period")).toBeTruthy();
  });
});

describe("measure", () => {
  it("formats a value with its unit and falls back to unknown", () => {
    expect(measure(20000, "mm/s²")).toBe("20000 mm/s²");
    expect(measure(0.5, "", 2)).toBe("0.5");
    expect(measure(null, "mm/s")).toBe("unknown");
  });
});
