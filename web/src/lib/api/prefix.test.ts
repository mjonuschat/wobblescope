import { beforeEach, describe, expect, it } from "vitest";

import { holdUrl } from "@/pages/stream/useHold";
import { liveUrl } from "@/pages/run/useLive";
import { streamUrl } from "@/stream/useStream";
import { setBase, stubFetch } from "@/test/http";

import { apiJson } from "./client";
import { sessionDownloadUrl } from "./endpoints";

// Mainsail's sidebar opens Wobblescope at /wobblescope/ and nginx strips that prefix, so every
// request and socket must resolve against the page and none against the host root.
describe("a page served under /wobblescope/", () => {
  beforeEach(() => setBase("http://printer.local/wobblescope/"));

  it("sends API requests below the prefix", async () => {
    const http = stubFetch({ body: {} });
    await apiJson("status");
    await apiJson("sessions/s1/results");
    expect(http.url(0)).toBe("http://printer.local/wobblescope/api/status");
    expect(http.url(1)).toBe("http://printer.local/wobblescope/api/sessions/s1/results");
  });

  it("opens the live, stream and hold sockets below the prefix", () => {
    expect(liveUrl()).toBe("ws://printer.local/wobblescope/api/live");
    expect(streamUrl()).toBe("ws://printer.local/wobblescope/api/stream");
    expect(holdUrl()).toBe("ws://printer.local/wobblescope/api/hold");
  });

  it("links downloads below the prefix and encodes the id", () => {
    expect(sessionDownloadUrl("a b")).toBe("http://printer.local/wobblescope/api/sessions/a%20b/download");
  });
});
