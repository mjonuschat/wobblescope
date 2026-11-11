import { useEffect, useMemo, useState } from "react";

import { EChart } from "@/components/charts/EChart";
import type { TestState } from "@/lib/generated/TestState";

import { NO_TRACES, type Traces, extend, liveSignalOption } from "./trace";

/** The signal of each chip against frequency, drawn as the sweep goes. */
export function LiveChart({ test }: { test: TestState }) {
  const [traces, setTraces] = useState<Traces>(NO_TRACES);
  useEffect(() => {
    setTraces((old) => extend(old, test));
  }, [test]);
  const option = useMemo(() => liveSignalOption(traces.series), [traces]);
  return <EChart option={option} label="Signal against frequency for each chip" height={260} />;
}
