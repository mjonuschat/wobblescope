import { type ReactNode, useMemo } from "react";

import { EXPANDED_CHART_HEIGHT, ExpandDialog } from "@/components/ExpandDialog";
import { Panel } from "@/ui-kit/Panel";
import { EChart } from "@/components/charts/EChart";
import { WIDE_CHART } from "@/components/charts/size";
import { summedOption } from "@/components/charts/options";
import type { SummedPsd } from "@/lib/generated/SummedPsd";
import { useChartTheme } from "@/ui-kit/chartTheme";

interface Props {
  /** Links the chart's tooltip and legend with the other charts of the group. */
  linkGroup?: string;
  /** Where the vibration axis ends, so two charts can share it. */
  max?: number;
  summed: SummedPsd;
  maxFreq: number;
  switcher: ReactNode;
}

const LABEL = "All chips added together, vibration strength against frequency";

export function CombinedResult({ summed, maxFreq, switcher, linkGroup, max }: Props) {
  const theme = useChartTheme();
  // The same option on a re-render keeps the legend choices.
  const option = useMemo(() => summedOption(summed, maxFreq, theme, max), [summed, maxFreq, theme, max]);
  return (
    <Panel title="Resonance and shapers" label="All chips">
      {switcher}
      <div className="relative">
        <EChart option={option} label={LABEL} minWidth={WIDE_CHART} legendSwitches={["Axes"]} group={linkGroup} />
        <div className="absolute top-0 right-0">
          <ExpandDialog title="Resonance and shapers: All chips">
            <EChart option={option} label={LABEL} height={EXPANDED_CHART_HEIGHT} legendSwitches={["Axes"]} />
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
