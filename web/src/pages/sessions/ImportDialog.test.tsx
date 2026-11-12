import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { chipsSnapshot } from "@/test/capture";
import type { Reply } from "@/test/http";
import { renderRoute } from "@/test/render";
import { stubApiWithStatus as stubApi } from "@/test/status";
import { summary } from "@/test/summaries";

const X1 = "raw_data_x_adxl345_20261111_045236.csv";
const Y1 = "raw_data_y_adxl345_20261111_045236.csv";
const Y2 = "raw_data_y_adxl345_20261111_050000.csv";
const X2 = "raw_data_x_adxl345_20261111_050000.csv";

let modified = 1;
const csv = (name: string) => new File(["1,2,3,4"], name, { type: "text/csv", lastModified: modified++ });

function api(importReply: (index: number) => Reply = (i) => ({ status: 201, body: summary(`csv-${i + 1}`) })) {
  let imports = 0;
  return stubApi(({ url, method }) => {
    if (method === "POST") {
      const reply = importReply(imports);
      imports += 1;
      return reply;
    }
    if (url.pathname.endsWith("/chips")) return { body: chipsSnapshot() };
    if (url.pathname.endsWith("/storage")) return { status: 503, body: { error: "storage_unavailable", message: "x" } };
    if (url.pathname.endsWith("/sessions")) return { body: { sessions: [summary("s1")] } };
    return { status: 404, body: { error: "not_found", message: "no" } };
  });
}

async function openDialog() {
  const { router } = renderRoute("/sessions");
  fireEvent.click(await screen.findByRole("button", { name: "Import CSV" }));
  await screen.findByRole("dialog");
  await waitFor(() => expect(document.querySelector("[data-slot=dialog-content]")).toBeTruthy());
  return router;
}

const inDialog = () => within(screen.getByRole("dialog"));
const addFiles = (...names: string[]) =>
  fireEvent.change(inDialog().getByLabelText("CSV files"), { target: { files: names.map(csv) } });
const cards = () => inDialog().queryAllByRole("region", { name: /^Group \d+$/ });
const rows = () => [...document.querySelectorAll<HTMLElement>("[data-slot=attachment]")];
const rowOf = (name: string) => rows().find((r) => r.textContent?.includes(name)) as HTMLElement;
const importButton = () => screen.getByRole("button", { name: "Import" }) as HTMLButtonElement;
const posts = <T extends { method: string }>(calls: T[]) => calls.filter((c) => c.method === "POST");

async function sessionPart(call: { body: unknown }) {
  const form = call.body as FormData;
  return JSON.parse(await (form.get("session") as Blob).text());
}

describe("the import dialog", () => {
  it("shows a dropzone and no groups at first", async () => {
    api();
    await openDialog();
    expect(inDialog().getByLabelText("CSV files")).toBeTruthy();
    expect(cards()).toHaveLength(0);
    expect(importButton().disabled).toBe(true);
  });

  it("dropping two files with one stamp shows one group with two rows", async () => {
    api();
    await openDialog();
    fireEvent.drop(inDialog().getByRole("group", { name: "CSV drop area" }), {
      dataTransfer: { files: [csv(X1), csv(Y1)] },
    });
    await waitFor(() => expect(cards()).toHaveLength(1));
    expect(rows()).toHaveLength(2);
  });

  it("dropping files with different stamps shows separate groups and a file with no stamp its own group", async () => {
    api();
    await openDialog();
    addFiles(X1, X2, "capture.csv");
    await waitFor(() => expect(cards()).toHaveLength(3));
  });

  it("a refused file shows its reason and is not listed", async () => {
    api();
    await openDialog();
    addFiles("notes.txt");
    expect(await inDialog().findByText(/notes\.txt/)).toBeTruthy();
    expect(inDialog().getByText(/Only \.csv files can be imported/)).toBeTruthy();
    expect(rows()).toHaveLength(0);
  });
});

describe("a file row", () => {
  it("the chip is prefilled from the chips defaults and editing it clears the guess mark", async () => {
    api();
    await openDialog();
    addFiles("raw_data_x_20261111_045236.csv");
    await waitFor(() => expect(rows()).toHaveLength(1));
    await waitFor(() => expect(rows()[0].textContent).toContain("lis2dw ebb36"));
    expect(within(rows()[0]).getByText("guess")).toBeTruthy();
    fireEvent.click(within(rows()[0]).getByRole("button", { name: /^Edit/ }));
    fireEvent.change(await screen.findByLabelText("Chip"), { target: { value: "adxl345" } });
    expect(rows()[0].textContent).toContain("adxl345");
    expect(within(rows()[0]).queryByText("guess")).toBeNull();
  });

  it("Other… reveals a text field", async () => {
    api();
    await openDialog();
    addFiles(X1);
    await waitFor(() => expect(rows()).toHaveLength(1));
    fireEvent.click(within(rows()[0]).getByRole("button", { name: /^Edit/ }));
    expect(screen.queryByLabelText("Chip name")).toBeNull();
    fireEvent.change(await screen.findByLabelText("Chip"), { target: { value: "__other__" } });
    fireEvent.change(screen.getByLabelText("Chip name"), { target: { value: "adxl345 nh36" } });
    expect(rows()[0].textContent).toContain("adxl345 nh36");
  });

  it("editing the axis and point updates the row description", async () => {
    api();
    await openDialog();
    addFiles(X1);
    await waitFor(() => expect(rows()).toHaveLength(1));
    fireEvent.click(within(rows()[0]).getByRole("button", { name: /^Edit/ }));
    fireEvent.change(await screen.findByLabelText("Axis"), { target: { value: "Y" } });
    fireEvent.change(screen.getByLabelText("Point"), { target: { value: "10, 20, 30" } });
    expect(rows()[0].textContent).toContain("Y");
    expect(rows()[0].textContent).toContain("10, 20, 30");
  });
});

describe("groups", () => {
  it("moving a file to a new group creates it", async () => {
    api();
    await openDialog();
    addFiles(X1, Y1);
    await waitFor(() => expect(rows()).toHaveLength(2));
    fireEvent.change(within(rowOf(Y1)).getByLabelText(`Move ${Y1}`), { target: { value: "new" } });
    await waitFor(() => expect(cards()).toHaveLength(2));
  });

  it("removing a file removes an emptied group", async () => {
    api();
    await openDialog();
    addFiles(X1, X2);
    await waitFor(() => expect(cards()).toHaveLength(2));
    fireEvent.click(within(rowOf(X2)).getByRole("button", { name: `Remove ${X2}` }));
    await waitFor(() => expect(cards()).toHaveLength(1));
  });

  it("a group without a shared run name keeps Import disabled until the box is ticked", async () => {
    api();
    await openDialog();
    addFiles(X1, Y2);
    await waitFor(() => expect(cards()).toHaveLength(2));
    fireEvent.change(within(rowOf(Y2)).getByLabelText(`Move ${Y2}`), {
      target: { value: cards()[0].getAttribute("data-group-id") },
    });
    await waitFor(() => expect(cards()).toHaveLength(1));
    expect(inDialog().getByText("These files do not share a run name")).toBeTruthy();
    expect(importButton().disabled).toBe(true);
    fireEvent.click(inDialog().getByRole("checkbox", { name: "I recorded these together" }));
    await waitFor(() => expect(importButton().disabled).toBe(false));
  });

  it("a duplicate chip axis and point marks the later row as an error", async () => {
    api();
    await openDialog();
    addFiles(X1, "raw_data_x_adxl345_20261111_045236_again.csv");
    await waitFor(() => expect(rows()).toHaveLength(2));
    expect(rows()[0].getAttribute("data-state")).toBe("done");
    expect(rows()[1].getAttribute("data-state")).toBe("error");
    expect(rows()[1].textContent).toContain("same chip, axis and point");
    expect(importButton().disabled).toBe(true);
  });
});

describe("importing", () => {
  it("sends one multipart request per group in order, with the group's date as created_at", async () => {
    const calls = api();
    await openDialog();
    addFiles(X1, X2);
    await waitFor(() => expect(cards()).toHaveLength(2));
    fireEvent.click(importButton());
    await waitFor(() => expect(posts(calls)).toHaveLength(2));
    const [first, second] = posts(calls);
    expect((await sessionPart(first)).created_at).toBe(new Date(2026, 10, 11, 4, 52, 36).toISOString());
    expect((await sessionPart(second)).created_at).toBe(new Date(2026, 10, 11, 5, 0, 0).toISOString());
    expect(((first.body as FormData).getAll("file")[0] as File).name).toBe(X1);
  });

  it("changing the time of a group is sent as its created_at", async () => {
    const calls = api();
    await openDialog();
    addFiles(X1);
    await waitFor(() => expect(cards()).toHaveLength(1));
    fireEvent.click(inDialog().getByRole("button", { name: "Date and time" }));
    fireEvent.change(await screen.findByLabelText("Time"), { target: { value: "06:07:08" } });
    fireEvent.click(importButton());
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect((await sessionPart(posts(calls)[0])).created_at).toBe(new Date(2026, 10, 11, 6, 7, 8).toISOString());
  });

  it("clearing the time field keeps the earlier created_at", async () => {
    const calls = api();
    await openDialog();
    addFiles(X1);
    await waitFor(() => expect(cards()).toHaveLength(1));
    fireEvent.click(inDialog().getByRole("button", { name: "Date and time" }));
    const time = await screen.findByLabelText("Time");
    fireEvent.change(time, { target: { value: "06:07:08" } });
    fireEvent.change(time, { target: { value: "" } });
    fireEvent.click(importButton());
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect((await sessionPart(posts(calls)[0])).created_at).toBe(new Date(2026, 10, 11, 6, 7, 8).toISOString());
  });

  it("while a group is being sent the dropzone and Cancel are disabled and Escape does not close the dialog", async () => {
    api();
    const stubbed = globalThis.fetch;
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
      if (init?.method === "POST") await gate;
      return stubbed(input, init);
    });
    await openDialog();
    addFiles(X1);
    await waitFor(() => expect(cards()).toHaveLength(1));
    fireEvent.click(importButton());
    await waitFor(() => expect((inDialog().getByLabelText("CSV files") as HTMLInputElement).disabled).toBe(true));
    expect((inDialog().getByRole("button", { name: "Cancel" }) as HTMLButtonElement).disabled).toBe(true);
    expect((within(rowOf(X1)).getByRole("button", { name: `Remove ${X1}` }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.getByRole("dialog")).toBeTruthy();
    release();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("a single file imports through the same dialog", async () => {
    const calls = api();
    await openDialog();
    addFiles(X1);
    await waitFor(() => expect(cards()).toHaveLength(1));
    fireEvent.click(importButton());
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect((await sessionPart(posts(calls)[0])).files).toEqual([{ chip: "adxl345", axis: "x", point: null }]);
  });

  it("one stored session opens it", async () => {
    api();
    const router = await openDialog();
    addFiles(X1, Y1);
    await waitFor(() => expect(rows()).toHaveLength(2));
    fireEvent.click(importButton());
    await waitFor(() => expect(router.state.location.pathname).toBe("/sessions/csv-1"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("several stored sessions close the dialog and show Imported N sessions", async () => {
    api();
    const router = await openDialog();
    addFiles(X1, X2);
    await waitFor(() => expect(cards()).toHaveLength(2));
    fireEvent.click(importButton());
    expect(await screen.findByText("Imported 2 sessions")).toBeTruthy();
    expect(router.state.location.pathname).toBe("/sessions");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("a daemon error that names a file shows its message on the group and on the second attachment, in the error state", async () => {
    api(() => ({ status: 400, body: { error: "invalid_csv", message: "line 3 is not a sample", file: 2 } }));
    await openDialog();
    addFiles(X1, Y1);
    await waitFor(() => expect(rows()).toHaveLength(2));
    fireEvent.click(importButton());
    await waitFor(() => expect(rows()[1].getAttribute("data-state")).toBe("error"));
    expect(rows()[0].getAttribute("data-state")).toBe("done");
    expect(within(rows()[1]).getByText("line 3 is not a sample")).toBeTruthy();
    expect(within(cards()[0]).getAllByText("line 3 is not a sample").length).toBeGreaterThan(1);
  });

  it("when one group fails the dialog stays on that group with the error and does not navigate, and a toast says Imported 1 session with an Open action", async () => {
    api((i) =>
      i === 0
        ? { status: 201, body: summary("csv-1") }
        : { status: 400, body: { error: "invalid_request", message: "files overlap" } },
    );
    const router = await openDialog();
    addFiles(X1, X2);
    await waitFor(() => expect(cards()).toHaveLength(2));
    fireEvent.click(importButton());
    expect(await screen.findByText("Imported 1 session")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open" })).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(cards()).toHaveLength(1);
    expect(within(cards()[0]).getByText("files overlap")).toBeTruthy();
    expect(rowOf(X2)).toBeTruthy();
    expect(router.state.location.pathname).toBe("/sessions");
  });
});
