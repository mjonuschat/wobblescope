import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { renderRoute } from "@/test/render";
import { stubApiWithStatus as stubApi } from "@/test/status";
import { summary, usage } from "@/test/summaries";

function api() {
  return stubApi(({ url }) => {
    const path = url.pathname.replace(/^.*\/api\//, "");
    if (path === "storage") return { body: usage };
    if (path === "sessions") return { body: { sessions: [summary("s1")] } };
    if (path === "results") return { body: { results: [] } };
    return { status: 404, body: { error: "not_found", message: "no" } };
  });
}

describe("the history page", () => {
  it("switches between sessions and results and keeps the storage use mounted", async () => {
    const calls = api();
    renderRoute("/sessions");
    const storage = await screen.findByRole("region", { name: "Storage use" });
    expect(screen.getByRole("heading", { name: "History" })).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "Results" }));
    expect(await screen.findByText(/No saved results yet/)).toBeTruthy();
    expect(screen.getByRole("region", { name: "Storage use" })).toBe(storage);
    fireEvent.click(screen.getByRole("tab", { name: "Sessions" }));
    expect(await screen.findByText("s1")).toBeTruthy();
    expect(screen.getByRole("region", { name: "Storage use" })).toBe(storage);
    expect(calls.filter((c) => c.url.pathname.endsWith("/storage"))).toHaveLength(1);
  });
});
