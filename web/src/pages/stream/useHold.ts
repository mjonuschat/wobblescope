import { useCallback, useEffect, useRef, useState } from "react";

import type { CheckFailure } from "@/lib/generated/CheckFailure";
import type { HoldAxis } from "@/lib/generated/HoldAxis";
import type { HoldClientMessage } from "@/lib/generated/HoldClientMessage";
import type { HoldEnded } from "@/lib/generated/HoldEnded";
import type { HoldRequest } from "@/lib/generated/HoldRequest";
import type { HoldServerMessage } from "@/lib/generated/HoldServerMessage";
import type { HoldState } from "@/lib/generated/HoldState";

/** How often the page tells the service it is still there, in milliseconds. */
export const PING_INTERVAL_MS = 500;

/** What the page shows when the socket goes away without the service saying the hold ended. */
const LOST = "The connection to the service was lost. The service stops the printer within a few seconds.";

export type HoldStatus = "idle" | "starting" | "running" | "ended";

export interface HoldApi {
  status: HoldStatus;
  /** The hold's id, which the emergency-stop endpoint takes. */
  id: string | null;
  state: HoldState | null;
  /** Why the last change was not applied, until the next change. */
  rejected: string | null;
  /** The failed checks of a refused start. */
  failures: CheckFailure[];
  /** How the hold ended, when the service said so. */
  ended: HoldEnded | null;
  /** Set when the connection was lost without the service saying how the hold ended. */
  error: string | null;
  start: (request: HoldRequest) => void;
  set: (change: { frequency?: number; axis?: HoldAxis }) => void;
  stop: () => void;
}

interface View {
  status: HoldStatus;
  id: string | null;
  state: HoldState | null;
  rejected: string | null;
  failures: CheckFailure[];
  ended: HoldEnded | null;
  error: string | null;
}

const IDLE: View = {
  status: "idle",
  id: null,
  state: null,
  rejected: null,
  failures: [],
  ended: null,
  error: null,
};

/** The websocket URL. It is `api/hold` below the document base, with `ws` or `wss`. */
export function holdUrl(): string {
  const url = new URL("api/hold", document.baseURI);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return url.href;
}

function parse(data: unknown): HoldServerMessage | null {
  if (typeof data !== "string") return null;
  try {
    const value: unknown = JSON.parse(data);
    if (typeof value === "object" && value !== null && "type" in value) return value as HoldServerMessage;
  } catch {
    // Not JSON, so ignore the message.
  }
  return null;
}

/**
 * The control socket of a held excitation. `start` opens it and sends the request, and while it is
 * open the hook pings every half second so the service stops the printer when the page goes away.
 */
export function useHold(createSocket: (url: string) => WebSocket = (url) => new WebSocket(url)): HoldApi {
  const [view, setView] = useState<View>(IDLE);
  const socketRef = useRef<WebSocket | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const statusRef = useRef<HoldStatus>("idle");

  const apply = useCallback((next: Partial<View>) => {
    if (next.status) statusRef.current = next.status;
    setView((old) => ({ ...old, ...next }));
  }, []);

  const closeSocket = useCallback(() => {
    if (timerRef.current !== null) clearInterval(timerRef.current);
    timerRef.current = null;
    const socket = socketRef.current;
    socketRef.current = null;
    socket?.close();
  }, []);

  const send = useCallback((message: HoldClientMessage) => {
    const socket = socketRef.current;
    if (socket && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(message));
  }, []);

  const start = useCallback(
    (request: HoldRequest) => {
      if (socketRef.current) return;
      apply({ ...IDLE, status: "starting" });
      const socket = createSocket(holdUrl());
      socketRef.current = socket;
      socket.onopen = () => {
        send({ type: "start", ...request });
        timerRef.current = setInterval(() => send({ type: "ping" }), PING_INTERVAL_MS);
      };
      socket.onmessage = (event: MessageEvent) => {
        const message = parse(event.data);
        if (!message) return;
        switch (message.type) {
          case "started":
            apply({ status: "running", id: message.id });
            break;
          case "state": {
            const { type: _type, ...state } = message;
            apply({ state });
            break;
          }
          case "rejected":
            apply({ rejected: message.message });
            break;
          case "refused":
            apply({ status: "idle", failures: message.failures });
            closeSocket();
            break;
          case "ended": {
            const { type: _type, ...ended } = message;
            apply({ status: "ended", ended });
            closeSocket();
            break;
          }
        }
      };
      socket.onclose = () => {
        if (socketRef.current !== socket) return;
        closeSocket();
        if (statusRef.current === "starting" || statusRef.current === "running") {
          apply({ status: "ended", error: LOST });
        }
      };
    },
    [apply, closeSocket, createSocket, send],
  );

  const set = useCallback(
    (change: { frequency?: number; axis?: HoldAxis }) => {
      apply({ rejected: null });
      send({ type: "set", ...change } as HoldClientMessage);
    },
    [apply, send],
  );

  const stop = useCallback(() => send({ type: "stop" }), [send]);

  // Leaving the page stops a running hold. The service would stop it a moment later anyway.
  useEffect(
    () => () => {
      if (statusRef.current === "running" || statusRef.current === "starting") send({ type: "stop" });
      closeSocket();
    },
    [closeSocket, send],
  );

  return { ...view, start, set, stop };
}
