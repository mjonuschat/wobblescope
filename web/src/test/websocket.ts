import { vi } from "vitest";

/** A WebSocket the test drives: it records its URL and sends nothing on its own. */
export class FakeSocket {
  static instances: FakeSocket[] = [];
  readonly url: string;
  closed = false;
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: (() => void) | null = null;

  /** Whether a new socket opens and sends an idle snapshot on its own. */
  static idle = false;

  constructor(url: string | URL) {
    this.url = String(url);
    FakeSocket.instances.push(this);
    if (FakeSocket.idle) {
      queueMicrotask(() => {
        this.open();
        this.send({ test: null, last: null });
      });
    }
  }

  /** The daemon accepts the connection. */
  open() {
    this.onopen?.();
  }

  /** The daemon sends a snapshot. */
  send(value: unknown) {
    this.onmessage?.(new MessageEvent("message", { data: JSON.stringify(value) }));
  }

  /** The connection drops. */
  drop() {
    this.closed = true;
    this.onclose?.();
  }

  close() {
    this.closed = true;
  }

  static get latest(): FakeSocket {
    return FakeSocket.instances[FakeSocket.instances.length - 1];
  }
}

/**
 * Replaces `WebSocket` with [`FakeSocket`] and forgets earlier sockets. With `idle`, each socket
 * opens at once and reports no test, as a daemon with nothing running does.
 */
export function stubWebSocket({ idle = false }: { idle?: boolean } = {}) {
  FakeSocket.instances = [];
  FakeSocket.idle = idle;
  vi.stubGlobal("WebSocket", FakeSocket);
}
