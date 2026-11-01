import { vi } from 'vitest'

export type Reply = { status?: number; body?: unknown; text?: string; bytes?: ArrayBuffer }

/** Replaces `fetch` with a stub that answers in order and records what it was asked. */
export function stubFetch(...replies: Reply[]) {
  const queue = [...replies]
  const fn = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => {
    const reply = queue.length > 1 ? queue.shift()! : queue[0]
    const status = reply.status ?? 200
    if (status === 204) return new Response(null, { status })
    const body =
      reply.bytes ?? reply.text ?? (reply.body === undefined ? '' : JSON.stringify(reply.body))
    return new Response(body, { status })
  })
  vi.stubGlobal('fetch', fn)
  return {
    fn,
    url: (i = 0) => String(fn.mock.calls[i][0]),
    init: (i = 0) => fn.mock.calls[i][1] as RequestInit,
    header: (i: number, name: string) => new Headers(fn.mock.calls[i][1]?.headers).get(name),
  }
}

export function setBase(href: string) {
  const base = document.createElement('base')
  base.href = href
  document.head.appendChild(base)
}
