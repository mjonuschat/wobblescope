import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'

import { queryKeys } from '@/lib/api/queries'
import type { FinishedTest } from '@/lib/generated/FinishedTest'
import type { LiveSnapshot } from '@/lib/generated/LiveSnapshot'
import type { TestState } from '@/lib/generated/TestState'

/** The wait before the first reconnect, doubled after each failed attempt. */
export const RECONNECT_START_MS = 500
/** The longest wait between reconnects. */
export const RECONNECT_MAX_MS = 8000

/** What the live socket currently tells the page. */
export interface LiveState {
  /** Whether the socket is open right now. */
  connected: boolean
  /** Whether any snapshot has arrived since the page opened. */
  known: boolean
  /** The running test, when there is one. */
  test: TestState | null
  /** The last test that ended. */
  last: FinishedTest | null
}

const INITIAL: LiveState = { connected: false, known: false, test: null, last: null }

/**
 * The websocket URL: `api/live` below the document base, with `ws` or `wss` for the page's
 * scheme, so a reverse-proxy path prefix is kept as it is for fetches.
 */
export function liveUrl(): string {
  const url = new URL('api/live', document.baseURI)
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:'
  return url.href
}

function parse(data: unknown): LiveSnapshot | null {
  if (typeof data !== 'string') return null
  try {
    const value: unknown = JSON.parse(data)
    if (typeof value === 'object' && value !== null && 'test' in value && 'last' in value) {
      return value as LiveSnapshot
    }
  } catch {
    // Not JSON: ignore the frame.
  }
  return null
}

/**
 * Follows the daemon's live socket. It reconnects with a growing wait and treats each snapshot
 * as the whole truth. A finished test refreshes the sessions list and the storage usage.
 */
export function useLive(): LiveState {
  const queryClient = useQueryClient()
  const [state, setState] = useState<LiveState>(INITIAL)
  const lastSeen = useRef<string | null | undefined>(undefined)

  useEffect(() => {
    let closed = false
    let socket: WebSocket | null = null
    let timer: ReturnType<typeof setTimeout> | undefined
    let wait = RECONNECT_START_MS

    const connect = () => {
      socket = new WebSocket(liveUrl())
      socket.onopen = () => {
        wait = RECONNECT_START_MS
      }
      socket.onmessage = (event: MessageEvent) => {
        const snapshot = parse(event.data)
        if (!snapshot) return
        setState({ connected: true, known: true, test: snapshot.test, last: snapshot.last })
        const id = snapshot.last?.id ?? null
        // The first snapshot only sets the baseline: a test that ended before the page opened
        // changed nothing the page has cached.
        if (lastSeen.current !== undefined && id !== lastSeen.current && id !== null) {
          void queryClient.invalidateQueries({ queryKey: queryKeys.sessionLists })
          void queryClient.invalidateQueries({ queryKey: queryKeys.storage })
        }
        lastSeen.current = id
      }
      socket.onclose = () => {
        if (closed) return
        setState((old) => ({ ...old, connected: false }))
        timer = setTimeout(connect, wait)
        wait = Math.min(wait * 2, RECONNECT_MAX_MS)
      }
    }
    connect()
    return () => {
      closed = true
      clearTimeout(timer)
      socket?.close()
    }
  }, [queryClient])

  return state
}
