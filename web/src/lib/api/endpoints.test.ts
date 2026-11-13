import type { SaveRequest } from "@/lib/generated/SaveRequest";
import { describe, expect, it, vi } from "vitest";

import type { ImportGroupState } from "@/pages/sessions/importModel";
import { setBase, stubFetch } from "@/test/http";

import { ApiClientError } from "./client";
import {
  checkChips,
  deleteSession,
  fetchResultSample,
  fetchSample,
  getChips,
  getSettings,
  importGroup,
  listSessions,
  pinSession,
  saveResult,
  startTest,
} from "./endpoints";

describe("endpoints", () => {
  it("lists with filters as query parameters", async () => {
    setBase("http://printer.local/wobblescope/");
    const http = stubFetch({ body: { sessions: [] } });
    await listSessions({ chip: "adxl345 nh36", pinned: true });
    expect(http.url()).toBe("http://printer.local/wobblescope/api/sessions?chip=adxl345+nh36&pinned=true");
  });

  it("sends a date range as the start of the first day and the start of the day after the last", async () => {
    const http = stubFetch({ body: { sessions: [] } });
    await listSessions({ createdFrom: "2026-11-01", createdTo: "2026-11-30" });
    const url = new URL(http.url());
    expect(url.searchParams.get("created_from")).toBe(new Date(2026, 10, 1).toISOString());
    expect(url.searchParams.get("created_to")).toBe(new Date(2026, 11, 1).toISOString());
  });

  it("leaves out a date that is empty or not a day", async () => {
    const http = stubFetch({ body: { sessions: [] } });
    await listSessions({ createdFrom: "", createdTo: "soon" });
    expect(new URL(http.url()).search).toBe("");
  });

  it("pins with a JSON body", async () => {
    const http = stubFetch({ body: { id: "s1", pinned: true } });
    await pinSession("s1", true);
    expect(http.init().method).toBe("PATCH");
    expect(http.init().body).toBe('{"pinned":true}');
  });

  it("deletes with DELETE", async () => {
    const http = stubFetch({ status: 204 });
    await deleteSession("s1");
    expect(http.init().method).toBe("DELETE");
    expect(http.url()).toMatch(/\/api\/sessions\/s1$/);
  });

  it("downloads the exact bytes of one recording", async () => {
    const bytes = new Uint8Array([1, 2, 3, 250]).buffer;
    const http = stubFetch({ bytes });
    const got = await fetchSample("s1", 1, 0);
    expect(http.url()).toMatch(/\/api\/sessions\/s1\/samples\/1\/0$/);
    expect([...new Uint8Array(got)]).toEqual([1, 2, 3, 250]);
  });

  it("reads settings and chips with GET and checks chips with POST", async () => {
    const http = stubFetch({ body: {} });
    await getSettings();
    await getChips();
    await checkChips();
    expect(http.url(0)).toMatch(/\/api\/settings$/);
    expect(http.url(1)).toMatch(/\/api\/chips$/);
    expect(http.url(2)).toMatch(/\/api\/chips\/check$/);
    expect(http.init(2).method).toBe("POST");
    expect(http.header(2, "X-Wobblescope-Client")).toBe("1");
  });

  it("starts a test with a JSON body and stops it by id", async () => {
    const http = stubFetch({ status: 202, body: { id: "test-1" } });
    await startTest({
      chips: ["adxl345"],
      kind: "axes",
      axes: ["X"],
      mode: "Pulse",
      overrides: { freq_end: 100 },
      points: [[100, 100, 20]],
    });
    expect(http.init().method).toBe("POST");
    expect(http.header(0, "Content-Type")).toBe("application/json");
    expect(JSON.parse(String(http.init().body))).toEqual({
      chips: ["adxl345"],
      kind: "axes",
      axes: ["X"],
      mode: "Pulse",
      overrides: { freq_end: 100 },
      points: [[100, 100, 20]],
    });
  });

  it("surfaces a preflight_failed answer as an error that carries every failed check", async () => {
    stubFetch({
      status: 409,
      body: {
        error: "preflight_failed",
        message: "2 checks failed",
        failures: [
          { check: "homed", message: "home X, Y and Z first" },
          { check: "klipper_ready", message: "Klipper is shutdown, not ready" },
        ],
      },
    });
    const error = await startTest({
      chips: ["adxl345"],
      kind: "axes",
      axes: ["X"],
      mode: "Pulse",
      overrides: {},
      points: [[1, 2, 3]],
    }).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiClientError);
    const body = (error as ApiClientError).body;
    expect(body?.error).toBe("preflight_failed");
    expect(body?.error === "preflight_failed" && body.failures.map((f) => f.check)).toEqual(["homed", "klipper_ready"]);
  });

  it("saveResult posts the JSON with the client header", async () => {
    const http = stubFetch({ status: 201, body: { id: "r1" } });
    const body = { label: "after belt swap" } as unknown as SaveRequest;
    await saveResult("s 1", body);
    expect(http.url()).toMatch(/\/api\/sessions\/s%201\/results$/);
    expect(http.init().method).toBe("POST");
    expect(http.init().body).toBe(JSON.stringify(body));
    expect(new Headers(http.init().headers).get("Content-Type")).toBe("application/json");
    expect(http.header(0, "X-Wobblescope-Client")).toBe("1");
  });

  it("fetchResultSample requests results/{id}/samples/{chip}/{segment}", async () => {
    const http = stubFetch({ bytes: new Uint8Array([7]).buffer });
    const got = await fetchResultSample("r1", 1, 2);
    expect(http.url()).toMatch(/\/api\/results\/r1\/samples\/1\/2$/);
    expect([...new Uint8Array(got)]).toEqual([7]);
  });

  const groupOf = (...entries: [string, string, "X" | "Y" | "", string][]): ImportGroupState => ({
    id: "g1",
    createdAt: new Date("2026-11-12T10:00:00Z"),
    confirmed: true,
    entries: entries.map(([name, chip, axis, point], i) => ({
      id: `e${i}`,
      file: new File([name], name),
      chip,
      chipGuessed: false,
      axis,
      point,
      stampToken: null,
    })),
  });

  it("importGroup posts multipart with the session part first and the files in order", async () => {
    const http = stubFetch({ status: 201, body: { id: "csv-1" } });
    await importGroup(groupOf(["a.csv", "adxl345", "X", "1, 2, 3"], ["b.csv", "adxl345 nh36", "Y", ""]));
    expect(new URL(http.url()).pathname.endsWith("/api/imports")).toBe(true);
    expect(http.init().method).toBe("POST");
    expect(http.header(0, "X-Wobblescope-Client")).toBe("1");
    expect(http.header(0, "Content-Type")).toBeNull();
    const form = http.init().body as FormData;
    expect([...form.keys()]).toEqual(["session", "file", "file"]);
    const session = form.get("session") as Blob;
    expect(session.type).toBe("application/json");
    expect(JSON.parse(await session.text())).toEqual({
      created_at: "2026-11-12T10:00:00.000Z",
      files: [
        { chip: "adxl345", axis: "x", point: [1, 2, 3] },
        { chip: "adxl345 nh36", axis: "y", point: null },
      ],
    });
    expect(form.getAll("file").map((f) => (f as File).name)).toEqual(["a.csv", "b.csv"]);
  });

  it("importGroup sends a null point for an empty point text", async () => {
    const http = stubFetch({ status: 201, body: { id: "csv-1" } });
    await importGroup(groupOf(["a.csv", "adxl345", "X", "  "]));
    const session = (http.init().body as FormData).get("session") as Blob;
    expect(JSON.parse(await session.text()).files[0].point).toBeNull();
  });

  it("importGroup refuses an invalid point text without sending", async () => {
    const http = stubFetch({ status: 201, body: {} });
    expect(() => importGroup(groupOf(["a.csv", "adxl345", "X", "1,2"]))).toThrow("a.csv");
    expect(http.fn).not.toHaveBeenCalled();
  });

  it("reports the bytes of a sample download as they arrive", async () => {
    const chunks = [new Uint8Array([1, 2]), new Uint8Array([3])];
    const stream = new ReadableStream({
      start(controller) {
        for (const chunk of chunks) controller.enqueue(chunk);
        controller.close();
      },
    });
    const response = new Response(stream, { headers: { "Content-Length": "3" } });
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => response),
    );

    const calls: [number, number | null][] = [];
    const onBytes = (loaded: number, total: number | null) => calls.push([loaded, total]);
    const result = await fetchSample("s1", 1, 0, undefined, onBytes);

    expect([...new Uint8Array(result)]).toEqual([1, 2, 3]);
    expect(calls).toContainEqual([2, 3]);
    expect(calls[calls.length - 1]).toEqual([3, 3]);
  });

  it("reports an unknown total when the length is missing", async () => {
    const chunks = [new Uint8Array([1, 2]), new Uint8Array([3])];
    const stream = new ReadableStream({
      start(controller) {
        for (const chunk of chunks) controller.enqueue(chunk);
        controller.close();
      },
    });
    const response = new Response(stream);
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => response),
    );

    const calls: [number, number | null][] = [];
    const onBytes = (loaded: number, total: number | null) => calls.push([loaded, total]);
    const result = await fetchSample("s1", 1, 0, undefined, onBytes);

    expect([...new Uint8Array(result)]).toEqual([1, 2, 3]);
    expect(calls[calls.length - 1]).toEqual([3, null]);
  });

  it("fetchResultSample reports progress", async () => {
    const chunks = [new Uint8Array([1, 2]), new Uint8Array([3])];
    const stream = new ReadableStream({
      start(controller) {
        for (const chunk of chunks) controller.enqueue(chunk);
        controller.close();
      },
    });
    const response = new Response(stream, { headers: { "Content-Length": "3" } });
    const fetchMock = vi.fn(async () => response);
    vi.stubGlobal("fetch", fetchMock);

    const calls: [number, number | null][] = [];
    const onBytes = (loaded: number, total: number | null) => calls.push([loaded, total]);
    const result = await fetchResultSample("r1", 1, 2, undefined, onBytes);

    expect([...new Uint8Array(result)]).toEqual([1, 2, 3]);
    expect(calls[calls.length - 1]).toEqual([3, 3]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const lastCall = fetchMock.mock.lastCall as unknown as [RequestInfo | URL, RequestInit?];
    expect(new URL(String(lastCall[0])).pathname).toMatch(/\/api\/results\/r1\/samples\/1\/2$/);
  });
});
