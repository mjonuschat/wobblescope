import { useEffect, useMemo, useState } from "react";

import { EChart } from "@/components/charts/EChart";
import type { TestState } from "@/lib/generated/TestState";

import { NO_TRACES, type Traces, extend, liveSignalOption } from "./trace";

export function LiveChart({ test }: { test: TestState }) {
  const [traces, setTraces] = useState<Traces>(NO_TRACES);
  useEffect(() => {
    setTraces((old) => extend(old, test));
  }, [test]);
  const [low, high] = test.frequency_range;
  const option = useMemo(() => liveSignalOption(traces.series, [low, high]), [traces, low, high]);
  return <EChart option={option} label="Signal against frequency for each chip" height={260} />;
}
