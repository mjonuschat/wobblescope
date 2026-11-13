import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { Reply } from "@/test/http";
import { stubApiWithStatus as stubApi } from "@/test/status";
import { queryKeys } from "@/lib/api/queries";
import type { StatusResponse } from "@/lib/generated/StatusResponse";
import { renderRoute } from "@/test/render";
import { healthyStatus } from "@/test/status";
import { summary, usage } from "@/test/summaries";

const waiting = { error: "storage_unavailable", message: "waiting for Moonraker" };

function api(
  sessions = [summary("s1"), summary("s2")],
  extra: (path: string, method: string) => Reply | null = () => null,
) {
  return stubApi(({ url, method }) => {
    const path = url.pathname.replace(/^.*\/api\//, "");
    const custom = extra(path, method);
    if (custom) return custom;
    if (path === "storage") return { body: usage };
    if (path === "sessions") return { body: { sessions } };
    return { status: 404, body: { error: "not_found", message: "no" } };
  });
}

describe("the sessions page", () => {
  it("lists sessions in the order returned with chips and size, and outlines the unpin button of a pinned one", async () => {
    api([
      summary("newest", { chips: ["adxl345", "lis2dw ebb36"], size_bytes: 1536, pinned: true }),
      summary("older", { size_bytes: 3 * 1024 ** 2 }),
      summary("oldest", { source: "CsvImport", mode: null }),
    ]);
    renderRoute("/sessions");
    const rows = await screen.findAllByRole("row");
    const text = rows.slice(1).map((r) => r.textContent);
    expect(text[0]).toContain("newest");
    expect(text[0]).toContain("adxl345, lis2dw ebb36");
    expect(text[0]).toContain("1.5 KiB");
    expect(within(rows[1]).getByRole("button", { name: "Unpin" }).className).toContain("border-primary");
    expect(within(rows[1]).queryByText("Pinned")).toBeNull();
    expect(text[1]).toContain("older");
    expect(text[1]).toContain("3 MiB");
    expect(within(rows[2]).getByRole("button", { name: "Pin" }).className).not.toContain("border-primary");
    expect(text[2]).toContain("CSV");
  });

  it("a_belt_session_is_tagged_belts", async () => {
    api([summary("belt", { kind: "belts" }), summary("axes"), summary("odd", { kind: "other" })]);
    renderRoute("/sessions");
    const rows = (await screen.findAllByRole("row")).slice(1);
    expect(within(rows[0]).getByText("Belts")).toBeTruthy();
    expect(within(rows[1]).queryByText("Belts")).toBeNull();
    expect(within(rows[2]).queryByText("Belts")).toBeNull();
  });

  it("links each row to the session's zip download", async () => {
    api([summary("s1")]);
    renderRoute("/sessions");
    const link = (await screen.findByLabelText("Download")) as HTMLAnchorElement;
    expect(link.getAttribute("href")).toMatch(/\/api\/sessions\/s1\/download$/);
    expect(link.hasAttribute("download")).toBe(true);
  });

  it('requests only pinned sessions when "pinned only" is chosen', async () => {
    const calls = api();
    renderRoute("/sessions");
    await screen.findAllByRole("row");
    fireEvent.click(screen.getByRole("button", { name: "Filter Pinned" }));
    fireEvent.click(await screen.findByRole("combobox", { name: "Pinned" }));
    const option = await screen.findByRole("option", { name: "Pinned Only" });
    fireEvent.pointerDown(option);
    fireEvent.pointerUp(option);
    fireEvent.click(option);
    await waitFor(() =>
      expect(
        calls.some((c) => c.url.pathname.endsWith("/sessions") && c.url.searchParams.get("pinned") === "true"),
      ).toBe(true),
    );
  });

  it("requests a date range picked on the calendar and clears it", async () => {
    const calls = api();
    renderRoute("/sessions");
    await screen.findAllByRole("row");
    fireEvent.click(screen.getByRole("button", { name: "Filter Created" }));
    const now = new Date();
    const day = (date: number) =>
      document.querySelector<HTMLElement>(
        `[data-day="${new Date(now.getFullYear(), now.getMonth(), date).toLocaleDateString()}"]`,
      );
    await waitFor(() => expect(day(3)).not.toBeNull());
    fireEvent.click(day(3) as HTMLElement);
    fireEvent.click(day(5) as HTMLElement);
    const ranged = () =>
      calls.some(
        (c) =>
          c.url.pathname.endsWith("/sessions") &&
          c.url.searchParams.get("created_from") === new Date(now.getFullYear(), now.getMonth(), 3).toISOString() &&
          c.url.searchParams.get("created_to") === new Date(now.getFullYear(), now.getMonth(), 6).toISOString(),
      );
    await waitFor(() => expect(ranged()).toBe(true));
    expect(screen.getByRole("button", { name: "Filter Created" }).className).toContain("text-primary");
    fireEvent.click(screen.getByRole("button", { name: "Clear dates" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Clear dates" })).toBeNull());
    expect(screen.getByRole("button", { name: "Filter Created" }).className).not.toContain("text-primary");
  });

  it("keeps the column filters when a filter matches nothing, and marks the active one", async () => {
    stubApi(({ url }) => {
      const path = url.pathname.replace(/^.*\/api\//, "");
      if (path === "storage") return { body: usage };
      if (path === "sessions") {
        return { body: { sessions: url.searchParams.get("pinned") === "true" ? [] : [summary("s1")] } };
      }
      return { status: 404, body: { error: "not_found", message: "no" } };
    });
    renderRoute("/sessions");
    await screen.findByText("s1");
    fireEvent.click(screen.getByRole("button", { name: "Filter Pinned" }));
    const option = await screen.findByRole("option", { name: "Pinned Only" }, { timeout: 100 }).catch(async () => {
      fireEvent.click(await screen.findByRole("combobox", { name: "Pinned" }));
      return screen.findByRole("option", { name: "Pinned Only" });
    });
    fireEvent.pointerDown(option);
    fireEvent.pointerUp(option);
    fireEvent.click(option);
    expect(await screen.findByText("No sessions match these filters.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Filter Pinned" }).className).toContain("text-primary");
  });

  it("pins an unpinned row with PATCH and shows it pinned afterwards", async () => {
    let pinned = false;
    const calls = api([], (path, method) => {
      if (path === "sessions" && method === "GET") return { body: { sessions: [summary("s1", { pinned })] } };
      if (path === "sessions/s1" && method === "PATCH") {
        pinned = true;
        return { body: summary("s1", { pinned: true }) };
      }
      return null;
    });
    renderRoute("/sessions");
    fireEvent.click(await screen.findByRole("button", { name: "Pin" }));
    expect(await screen.findByRole("button", { name: "Unpin" })).toBeTruthy();
    const patch = calls.find((c) => c.method === "PATCH")!;
    expect(patch.body).toBe('{"pinned":true}');
  });

  it("shows a failed pin on its row", async () => {
    api([summary("s1")], (_path, method) =>
      method === "PATCH" ? { status: 500, body: { error: "internal", message: "disk exploded" } } : null,
    );
    renderRoute("/sessions");
    fireEvent.click(await screen.findByRole("button", { name: "Pin" }));
    expect(await screen.findByText("disk exploded")).toBeTruthy();
  });

  it("covers AE2: a refused delete of a pinned session shows the refusal and keeps the row", async () => {
    api([summary("s1", { pinned: true })], (_path, method) =>
      method === "DELETE"
        ? { status: 409, body: { error: "pinned", message: "session s1 is pinned; unpin it first" } }
        : null,
    );
    renderRoute("/sessions");
    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    expect(await screen.findByText("session s1 is pinned; unpin it first")).toBeTruthy();
    expect(screen.getAllByRole("row", { hidden: true })).toHaveLength(2);
  });

  it("sends no DELETE when the confirmation is cancelled", async () => {
    const calls = api([summary("s1")]);
    renderRoute("/sessions");
    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(calls.some((c) => c.method === "DELETE")).toBe(false);
  });

  it("explains a daemon waiting for storage instead of showing a table", async () => {
    stubApi(({ url }) =>
      url.pathname.endsWith("/storage") || url.pathname.endsWith("/sessions")
        ? { status: 503, body: waiting }
        : { status: 404, body: { error: "not_found", message: "no" } },
    );
    renderRoute("/sessions");
    expect(await screen.findByText(/waiting for Moonraker to report where to store sessions/)).toBeTruthy();
    expect(screen.queryByRole("table")).toBeNull();
  });

  it("lists the sessions without a reload once the status says storage is open", async () => {
    let open = false;
    const waitingStatus: StatusResponse = { ...healthyStatus, storage: { state: "waiting" } };
    stubApi(
      ({ url }) =>
        open
          ? url.pathname.endsWith("/sessions")
            ? { body: { sessions: [summary("s1")] } }
            : { body: usage }
          : { status: 503, body: waiting },
      () => ({ body: open ? healthyStatus : waitingStatus }),
    );
    const { client } = renderRoute("/sessions");
    expect(await screen.findByText(/waiting for Moonraker to report where to store sessions/)).toBeTruthy();
    open = true;
    await client.invalidateQueries({ queryKey: queryKeys.status });
    expect(await screen.findByRole("table")).toBeTruthy();
    expect(screen.queryByText(/waiting for Moonraker to report/)).toBeNull();
  });

  it("recovers when the first status answer already says storage is open", async () => {
    let first = true;
    const calls = stubApi(({ url }) => {
      if (url.pathname.endsWith("/sessions")) {
        if (first) {
          first = false;
          return { status: 503, body: waiting };
        }
        return { body: { sessions: [summary("s1")] } };
      }
      return { body: usage };
    });
    renderRoute("/sessions");
    expect(await screen.findByRole("table")).toBeTruthy();
    expect(calls.filter((c) => c.url.pathname.endsWith("/sessions")).length).toBeGreaterThan(1);
  });

  it("does not fetch again a list that failed with another error code", async () => {
    const calls = stubApi(({ url }) =>
      url.pathname.endsWith("/sessions")
        ? { status: 500, body: { error: "internal", message: "disk on fire" } }
        : { body: usage },
    );
    renderRoute("/sessions");
    expect(await screen.findByText("disk on fire")).toBeTruthy();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(calls.filter((c) => c.url.pathname.endsWith("/sessions"))).toHaveLength(1);
  });

  it("shows loading, then an empty list with an Import action", async () => {
    api([]);
    renderRoute("/sessions");
    expect(await screen.findByText("Loading sessions…")).toBeTruthy();
    expect(await screen.findByText("No sessions yet.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Import a CSV" })).toBeTruthy();
  });

  it("shows a list failure with its message and a retry that fetches again", async () => {
    let fail = true;
    const calls = api([summary("s1")], (path) =>
      path === "sessions" && fail ? { status: 500, body: { error: "internal", message: "listing broke" } } : null,
    );
    renderRoute("/sessions");
    expect(await screen.findByText("listing broke")).toBeTruthy();
    fail = false;
    const before = calls.length;
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("s1")).toBeTruthy();
    expect(calls.length).toBeGreaterThan(before);
  });

  it("shows every storage figure and a bar of used plus reserved against the cap", async () => {
    api();
    renderRoute("/sessions");
    const region = await screen.findByRole("region", { name: "Storage use" });
    expect(region.textContent).toContain("1.6 GiB of 2 GiB used");
    expect(region.textContent).toContain("12 sessions");
    expect(region.textContent).toContain("100 MiB");
    expect(region.textContent).toContain("500 MiB");
    expect(region.textContent).toContain("10 GiB");
    expect(within(region).getByRole("progressbar").getAttribute("aria-valuenow")).toBe(
      String(usage.used_bytes + usage.reserved_bytes),
    );
  });

  it("puts the storage use under the list of sessions", async () => {
    api();
    renderRoute("/sessions");
    const table = await screen.findByRole("table");
    const storage = await screen.findByRole("region", { name: "Storage use" });
    expect(table.compareDocumentPosition(storage) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("labels each storage figure", async () => {
    api();
    renderRoute("/sessions");
    const region = await screen.findByRole("region", { name: "Storage use" });
    for (const label of ["Stored", "Reserved", "Pinned", "Free Disk"]) {
      expect(within(region).getByText(label)).toBeTruthy();
    }
  });

  it("explains in the tooltip what pinning does", async () => {
    api([summary("s1")]);
    renderRoute("/sessions");
    fireEvent.focus(await screen.findByRole("button", { name: "Pin" }));
    expect(await screen.findByText(/Never delete this session/)).toBeTruthy();
  });
});
