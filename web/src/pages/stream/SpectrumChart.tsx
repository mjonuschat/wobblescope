import { useMemo } from "react";

import { EChart } from "@/components/charts/EChart";
import { liveSpectrumOption } from "@/components/charts/options";

type Chips = Parameters<typeof liveSpectrumOption>[0]["chips"];

interface Props {
  chips: Chips;
  maxHz: number;
}

/** The live spectra of the chips. */
export function SpectrumChart({ chips, maxHz }: Props) {
  const option = useMemo(() => liveSpectrumOption({ chips, maxHz }), [chips, maxHz]);
  return <EChart option={option} label="Live spectrum of the streamed chips" height={420} />;
}
