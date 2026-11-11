import { vi } from "vitest";

/** A WebSocket the test drives: it records its URL and sends nothing on its own. */
export class FakeSocket {
  static readonly OPEN = 1;
  readyState = 0;
  static instances: FakeSocket[] = [];
  readonly url: string;
  closed = false;
  /** Text the page sent to the daemon. */
  sent: string[] = [];
  binaryType = "blob";
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: (() => void) | null = null;

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
    this.readyState = FakeSocket.OPEN;
    this.onopen?.();
  }

  /** The daemon sends a snapshot. A string is the page sending to the daemon, and is recorded. */
  send(value: unknown) {
    if (typeof value === "string") {
      this.sent.push(value);
      return;
    }
    this.onmessage?.(new MessageEvent("message", { data: JSON.stringify(value) }));
  }

  /** The daemon sends a binary frame. */
  sendBinary(buffer: ArrayBuffer) {
    this.onmessage?.(new MessageEvent("message", { data: buffer }));
  }

  /** The connection drops. */
  drop() {
    this.closed = true;
    this.readyState = 3;
    this.onclose?.();
  }

  close() {
    this.closed = true;
  }

  static get latest(): FakeSocket {
    return FakeSocket.instances[FakeSocket.instances.length - 1];
  }
}

export function stubWebSocket({ idle = false }: { idle?: boolean } = {}) {
  FakeSocket.instances = [];
  FakeSocket.idle = idle;
  vi.stubGlobal("WebSocket", FakeSocket);
}
