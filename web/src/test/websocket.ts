import { vi } from 'vitest'

/** A WebSocket the test drives: it records its URL and sends nothing on its own. */
export class FakeSocket {
  static instances: FakeSocket[] = []
  readonly url: string
  closed = false
  onopen: (() => void) | null = null
  onmessage: ((event: MessageEvent) => void) | null = null
  onclose: (() => void) | null = null

  constructor(url: string | URL) {
    this.url = String(url)
    FakeSocket.instances.push(this)
  }

  /** The daemon accepts the connection. */
  open() {
    this.onopen?.()
  }

  /** The daemon sends a snapshot. */
  send(value: unknown) {
    this.onmessage?.(new MessageEvent('message', { data: JSON.stringify(value) }))
  }

  /** The connection drops. */
  drop() {
    this.closed = true
    this.onclose?.()
  }

  close() {
    this.closed = true
  }

  static get latest(): FakeSocket {
    return FakeSocket.instances[FakeSocket.instances.length - 1]
  }
}

/** Replaces `WebSocket` with [`FakeSocket`] and forgets earlier sockets. */
export function stubWebSocket() {
  FakeSocket.instances = []
  vi.stubGlobal('WebSocket', FakeSocket)
}
