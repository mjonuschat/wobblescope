import { act, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { toast } from "sonner";

import { renderRoute } from "@/test/render";
import { stubApiWithStatus } from "@/test/status";
import { stubWebSocket } from "@/test/websocket";

describe("the root layout", () => {
  beforeEach(() => stubWebSocket({ idle: true }));

  it("dismisses_toasts_when_the_path_changes_but_not_for_a_search_change", async () => {
    stubApiWithStatus(() => ({ status: 404, body: { error: "not_found", message: "no" } }));
    const { router } = renderRoute("/");
    await screen.findByRole("link", { name: "Input shaper" });
    act(() => {
      toast("A message that stays");
    });
    expect(await screen.findByText("A message that stays")).toBeTruthy();
    await act(() => router.navigate({ to: "/", search: { x: 1 } as never }));
    expect(screen.queryByText("A message that stays")).toBeTruthy();
    await act(() => router.navigate({ to: "/stream" }));
    await waitFor(() => expect(screen.queryByText("A message that stays")).toBeNull());
  });
});
