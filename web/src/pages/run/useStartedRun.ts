import { useCallback, useEffect, useRef, useState } from "react";

import type { LiveState } from "./useLive";

/**
 * The id of the run the form just started, until the live socket names it. A snapshot that
 * arrives after the form posted and mentions neither a running test nor a finished one with that
 * id means the run is gone (the daemon restarted, or another client's run took over), so the
 * page must not keep waiting for it.
 */
export function useStartedRun(
  live: Pick<LiveState, "seq" | "test" | "last">,
): [string | null, (id: string | null) => void] {
  const [started, setStarted] = useState<{ id: string; seq: number } | null>(null);
  const seq = useRef(live.seq);
  seq.current = live.seq;
  const set = useCallback((id: string | null) => setStarted(id === null ? null : { id, seq: seq.current }), []);

  useEffect(() => {
    if (started === null || live.seq <= started.seq) return;
    if (live.test?.id !== started.id && live.last?.id !== started.id) setStarted(null);
  }, [started, live.seq, live.test, live.last]);

  return [started?.id ?? null, set];
}
