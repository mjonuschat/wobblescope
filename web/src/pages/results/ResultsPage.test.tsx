import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { renderRoute } from "@/test/render";
import { resultSummary } from "@/test/results";
import { stubApiWithStatus as stubApi } from "@/test/status";

function serve(results = [resultSummary("r2", { label: "new belts" }), resultSummary("r1")]) {
  let list = results;
  const calls = stubApi(({ url, method }) => {
    const path = url.pathname.replace(/^.*\/api\//, "");
    if (path === "results" && method === "GET") return { body: { results: list } };
    if (path.startsWith("results/") && method === "DELETE") {
      list = list.filter((r) => `results/${r.id}` !== path);
      return { status: 204 };
    }
    return { status: 404, body: { error: "not_found", message: "no" } };
  });
  return calls;
}

describe("the results page", () => {
  it("lists results newest first with label, session and size", async () => {
    serve([
      resultSummary("r2", { label: "new belts", session_id: "s7", size_bytes: 3 * 1024 ** 2 }),
      resultSummary("r1", { size_bytes: 1536 }),
    ]);
    renderRoute("/results");
    const rows = (await screen.findAllByRole("row")).slice(1);
    expect(rows[0].textContent).toContain("new belts");
    expect(rows[0].textContent).toContain("s7");
    expect(rows[0].textContent).toContain("3 MiB");
    expect(rows[1].textContent).toContain("r1");
    expect(rows[1].textContent).toContain("1.5 KiB");
    expect(within(rows[0]).getByRole("link", { name: "new belts" }).getAttribute("href")).toBe("/results/r2");
  });

  it("marks an unusable result with its reason and offers delete", async () => {
    serve([
      resultSummary("bad", {
        label: null,
        created_at: null,
        session_id: null,
        core_version: null,
        unusable: "the manifest does not match",
      }),
    ]);
    renderRoute("/results");
    const row = (await screen.findAllByRole("row"))[1];
    expect(row.textContent).toContain("Unusable: the manifest does not match");
    expect(within(row).queryByRole("link")).toBeNull();
    expect(within(row).getByRole("button", { name: "Delete" })).toBeTruthy();
  });

  it("deletes after confirmation", async () => {
    const calls = serve();
    renderRoute("/results");
    const row = (await screen.findAllByRole("row"))[2];
    fireEvent.click(within(row).getByRole("button", { name: "Delete" }));
    expect(calls.some((c) => c.method === "DELETE")).toBe(false);
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() =>
      expect(calls.some((c) => c.method === "DELETE" && c.url.pathname.endsWith("/results/r1"))).toBe(true),
    );
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    await waitFor(() => expect(screen.getAllByRole("row")).toHaveLength(2));
  });

  it("keeps the dialog open and shows the error when the delete fails", async () => {
    let fail = true;
    const calls = stubApi(({ url, method }) => {
      const path = url.pathname.replace(/^.*\/api\//, "");
      if (path === "results" && method === "GET") return { body: { results: [resultSummary("r1")] } };
      if (path === "results/r1" && method === "DELETE") {
        if (fail) return { status: 500, body: { error: "internal", message: "disk is read-only" } };
        return { status: 204 };
      }
      return { status: 404, body: { error: "not_found", message: "no" } };
    });
    renderRoute("/results");
    const row = (await screen.findAllByRole("row"))[1];
    fireEvent.click(within(row).getByRole("button", { name: "Delete" }));
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    expect((await within(dialog).findByRole("alert")).textContent).toContain("disk is read-only");
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    fail = false;
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(calls.filter((c) => c.method === "DELETE")).toHaveLength(2);
  });

  it("shows an empty state", async () => {
    serve([]);
    renderRoute("/results");
    expect(await screen.findByText(/No saved results yet/)).toBeTruthy();
  });
});
