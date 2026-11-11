import type { ReactNode } from "react";

import { EXPANDED_CHART_HEIGHT, ExpandDialog } from "@/components/ExpandDialog";
import { Panel } from "@/components/Panel";
import { EChart } from "@/components/charts/EChart";
import { summedOption } from "@/components/charts/options";
import type { SummedPsd } from "@/lib/generated/SummedPsd";

interface Props {
  summed: SummedPsd;
  maxFreq: number;
  /** The control that switches between chips, shown at the top of the card. */
  switcher: ReactNode;
}

const LABEL = "All chips added together, vibration strength against frequency";

/** The chips' vibration added up into one curve, for looking at how the printer vibrates overall. */
export function CombinedResult({ summed, maxFreq, switcher }: Props) {
  const option = summedOption(summed, maxFreq);
  return (
    <Panel title="Resonance and Shapers" label="All chips">
      {switcher}
      <div className="relative">
        <EChart option={option} label={LABEL} />
        <div className="absolute top-0 right-0">
          <ExpandDialog title="Resonance and Shapers: All Chips">
            <EChart option={option} label={LABEL} height={EXPANDED_CHART_HEIGHT} />
          </ExpandDialog>
        </div>
      </div>
      <p className="text-sm text-muted-foreground">
        This adds the chips together, to show how much the printer vibrates overall at each frequency. A chip with a
        stronger signal counts for more. It is only for looking: the recommendation comes from each chip&rsquo;s own
        data, not from this sum.
      </p>
    </Panel>
  );
}
