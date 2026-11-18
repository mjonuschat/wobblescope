import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import type { PrinterSettings } from "@/lib/generated/PrinterSettings";
import { printerSettings } from "@/test/capture";
import { renderRoute } from "@/test/render";
import { apiPath, stubApiWithStatus } from "@/test/status";
import { stubWebSocket } from "@/test/websocket";

function withKinematics(kinematics: string | null): PrinterSettings {
  const base = printerSettings();
  return { ...base, machine: { ...base.machine, kinematics } };
}

describe("the sidebar", () => {
  beforeEach(() => stubWebSocket({ idle: true }));

  it("offers_the_belts_page_only_on_corexy_and_limited_corexy", async () => {
    for (const [kinematics, offered] of [
      ["corexy", true],
      ["limited_corexy", true],
      ["cartesian", false],
      [null, false],
    ] as const) {
      stubApiWithStatus((call) =>
        apiPath(call) === "settings"
          ? { body: withKinematics(kinematics) }
          : { status: 404, body: { error: "not_found", message: "no" } },
      );
      renderRoute("/");
      await screen.findByRole("link", { name: "Input shaper" });
      if (offered) {
        expect(await screen.findByRole("link", { name: "Belts" })).toBeTruthy();
      } else {
        await waitFor(() => expect(screen.queryByRole("link", { name: "Belts" })).toBeNull());
      }
      cleanup();
    }
  });

  it("lists_the_vibrations_page_on_every_supported_kinematics", async () => {
    for (const [kinematics, offered] of [
      ["cartesian", true],
      ["limited_cartesian", true],
      ["corexy", true],
      ["limited_corexy", true],
      ["corexz", true],
      ["limited_corexz", true],
      ["delta", false],
      [null, false],
    ] as const) {
      stubApiWithStatus((call) =>
        apiPath(call) === "settings"
          ? { body: withKinematics(kinematics) }
          : { status: 404, body: { error: "not_found", message: "no" } },
      );
      renderRoute("/");
      await screen.findByRole("link", { name: "Input shaper" });
      if (offered) {
        expect(await screen.findByRole("link", { name: "Vibrations profile" })).toBeTruthy();
      } else {
        await waitFor(() => expect(screen.queryByRole("link", { name: "Vibrations profile" })).toBeNull());
      }
      cleanup();
    }
  });

  it("hides_the_belts_page_while_settings_are_not_available", async () => {
    stubApiWithStatus(() => ({ status: 404, body: { error: "not_found", message: "no" } }));
    renderRoute("/");
    await screen.findByRole("link", { name: "Input shaper" });
    expect(screen.queryByRole("link", { name: "Belts" })).toBeNull();
  });

  it("opens_the_page_links_from_a_menu_button_and_closes_them_on_navigation", async () => {
    stubApiWithStatus(() => ({ status: 404, body: { error: "not_found", message: "no" } }));
    renderRoute("/");
    await screen.findByRole("link", { name: "Input shaper" });
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Open menu" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("link", { name: "Input shaper" })).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("link", { name: "History" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
