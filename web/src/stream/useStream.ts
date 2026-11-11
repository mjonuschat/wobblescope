import { useCallback, useEffect, useRef, useState } from 'react'

import type { ClientMessage } from '@/lib/generated/ClientMessage'
import type { ServerMessage } from '@/lib/generated/ServerMessage'
import { RECONNECT_MAX_MS, RECONNECT_START_MS } from '@/pages/run/useLive'

import { type DecodedFrame, decodeFrame } from './frames'

export type Hello = Extract<ServerMessage, { type: 'hello' }>
export type Health = Extract<ServerMessage, { type: 'health' }>

/** The most console lines kept. */
const CONSOLE_LINES = 5

/** What the socket tells the page about one chip. */
export interface ChipStreamState {
  index: number
  rateHz: number
  inputRateHz: number
  configuredRateHz: number | null
  factor: number
  maxHz: number
  health: Health | null
  /** Why the stream ended, when it did. */
  closed: string | null
  /** Why the subscription was refused, when it was. */
  refused: string | null
}

export interface StreamState {
  connected: boolean
  hello: Hello | null
  chips: Record<string, ChipStreamState>
  console: string[]
}

export interface StreamApi {
  state: StreamState
  subscribe: (chip: string, maxHz: number, updatesPerSecond: number) => void
  unsubscribe: (chip: string) => void
  /** Registers a frame handler and returns the function that removes it. */
  onFrame: (handler: (chip: string, frame: DecodedFrame) => void) => () => void
}

const INITIAL: StreamState = { connected: false, hello: null, chips: {}, console: [] }

/** The websocket URL: `api/stream` below the document base, with `ws` or `wss`. */
export function streamUrl(): string {
  const url = new URL('api/stream', document.baseURI)
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:'
  return url.href
}

function parse(data: string): ServerMessage | null {
  try {
    const value: unknown = JSON.parse(data)
    if (typeof value === 'object' && value !== null && 'type' in value) return value as ServerMessage
  } catch {
    // Not JSON: ignore the message.
  }
  return null
}

/**
 * Follows the daemon's stream socket. It reconnects with a growing wait and sends the active
 * subscriptions again. Binary frames go to the registered handlers, not to React state.
 */
export function useStream(): StreamApi {
  const [state, setState] = useState<StreamState>(INITIAL)
  const socketRef = useRef<WebSocket | null>(null)
  const subscriptions = useRef(new Map<string, ClientMessage>())
  const handlers = useRef(new Set<(chip: string, frame: DecodedFrame) => void>())

  const send = useCallback((message: ClientMessage) => {
    const socket = socketRef.current
    if (socket && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(message))
  }, [])

  useEffect(() => {
    let closed = false
    let timer: ReturnType<typeof setTimeout> | undefined
    let wait = RECONNECT_START_MS
    const chipByIndex = new Map<number, string>()

    const handle = (message: ServerMessage) => {
      switch (message.type) {
        case 'hello':
          setState((old) => ({ ...old, hello: message }))
          break
        case 'stream':
          chipByIndex.set(message.index, message.chip)
          setState((old) => ({
            ...old,
            chips: {
              ...old.chips,
              [message.chip]: {
                index: message.index,
                rateHz: message.rate_hz,
                inputRateHz: message.input_rate_hz,
                configuredRateHz: message.configured_rate_hz,
                factor: message.factor,
                maxHz: message.max_hz,
                health: old.chips[message.chip]?.health ?? null,
                closed: null,
                refused: null,
              },
            },
          }))
          break
        case 'health':
          setState((old) => {
            const chip = old.chips[message.chip]
            return chip ? { ...old, chips: { ...old.chips, [message.chip]: { ...chip, health: message } } } : old
          })
          break
        case 'closed':
        case 'refused': {
          const field = message.type
          setState((old) => {
            const chip = old.chips[message.chip]
            const next = chip
              ? { ...chip, [field]: message.reason }
              : { index: -1, rateHz: 0, inputRateHz: 0, configuredRateHz: null, factor: 1, maxHz: 0, health: null, closed: null, refused: null, [field]: message.reason }
            return { ...old, chips: { ...old.chips, [message.chip]: next } }
          })
          break
        }
        case 'console':
          setState((old) => ({ ...old, console: [...old.console, ...message.lines].slice(-CONSOLE_LINES) }))
          break
      }
    }

    const connect = () => {
      const socket = new WebSocket(streamUrl())
      socket.binaryType = 'arraybuffer'
      socketRef.current = socket
      socket.onopen = () => {
        wait = RECONNECT_START_MS
        setState((old) => ({ ...old, connected: true }))
        for (const message of subscriptions.current.values()) socket.send(JSON.stringify(message))
      }
      socket.onmessage = (event: MessageEvent) => {
        if (typeof event.data === 'string') {
          const message = parse(event.data)
          if (message) handle(message)
          return
        }
        try {
          const frame = decodeFrame(event.data)
          const chip = chipByIndex.get(frame.index)
          if (chip === undefined) return
          for (const handler of handlers.current) handler(chip, frame)
        } catch {
          // A malformed frame is dropped.
        }
      }
      socket.onclose = () => {
        if (closed) return
        chipByIndex.clear()
        setState((old) => ({ ...old, connected: false }))
        timer = setTimeout(connect, wait)
        wait = Math.min(wait * 2, RECONNECT_MAX_MS)
      }
    }
    connect()
    return () => {
      closed = true
      clearTimeout(timer)
      socketRef.current?.close()
      socketRef.current = null
    }
  }, [])

  const subscribe = useCallback(
    (chip: string, maxHz: number, updatesPerSecond: number) => {
      const message: ClientMessage = { type: 'subscribe', chip, max_hz: maxHz, updates_per_second: updatesPerSecond }
      subscriptions.current.set(chip, message)
      send(message)
    },
    [send],
  )

  const unsubscribe = useCallback(
    (chip: string) => {
      subscriptions.current.delete(chip)
      send({ type: 'unsubscribe', chip })
      setState((old) => {
        const { [chip]: _gone, ...chips } = old.chips
        return { ...old, chips }
      })
    },
    [send],
  )

  const onFrame = useCallback((handler: (chip: string, frame: DecodedFrame) => void) => {
    handlers.current.add(handler)
    return () => {
      handlers.current.delete(handler)
    }
  }, [])

  return { state, subscribe, unsubscribe, onFrame }
}
