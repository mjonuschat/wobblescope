import type { EChartsCoreOption } from "echarts/core";
import { type ReactNode, useMemo } from "react";

import { EXPANDED_CHART_HEIGHT, ExpandDialog } from "@/components/ExpandDialog";
import { Field, Fields } from "@/ui-kit/Fields";
import { Panel } from "@/ui-kit/Panel";
import { EChart } from "@/components/charts/EChart";
import { Button } from "@/components/ui/button";
import { chipLabel } from "@/lib/format";
import type { ChipVibrations } from "@/lib/generated/ChipVibrations";
import type { MotorResult } from "@/lib/generated/MotorResult";
import type { VibrationsResult } from "@/lib/generated/VibrationsResult";

import { explainReason } from "../EligibilityPanel";
import { GoodAngleRanges } from "./VibrationsSummary";
import {
  LOW_FREQ_WARNING,
  angularProfilesOption,
  heatmapOption,
  motorProfileOption,
  polarAngleOption,
  polarHeatmapOption,
  speedProfileOption,
} from "./vibrationsOptions";
import { Alert } from "@/ui-kit/Alert";
import { useChartTheme } from "@/ui-kit/chartTheme";

/** The height of the two polar plots, which leaves room for their 15 degree labels. */
const POLAR_HEIGHT = 440;

/** The highest frequency of the motor profile in Hz. The analysis keeps the frequencies up to it. */
export const MAX_FREQ = 1000;

function ChartPanel({
  title,
  option,
  label,
  height = 380,
  intro,
  below,
}: {
  title: string;
  option: EChartsCoreOption;
  label: string;
  height?: number;
  intro?: ReactNode;
  /** What the chart's numbers refer to, shown under it and in the expanded view. */
  below?: ReactNode;
}) {
  return (
    <Panel
      title={title}
      label={title}
      action={
        <ExpandDialog title={title}>
          {intro}
          <EChart option={option} label={label} height={EXPANDED_CHART_HEIGHT} />
          {below}
        </ExpandDialog>
      }
    >
      {intro}
      <EChart option={option} label={label} height={height} />
      {below}
    </Panel>
  );
}

/** The motor resonance and damping ratio, which Shake&Tune prints inside its chart. */
function MotorFigures({ motor }: { motor: MotorResult }) {
  return (
    <div className="space-y-3">
      <Fields aria-label="Motor resonance">
        <Field label="Resonant frequency">{`${motor.fr.toFixed(1)} Hz`}</Field>
        <Field label="Damping ratio">{motor.zeta === null ? "Not available" : motor.zeta.toFixed(3)}</Field>
      </Fields>
      {motor.low_freq_warning && (
        <Alert tone="warning" variant="inline">
          {LOW_FREQ_WARNING}
        </Alert>
      )}
    </div>
  );
}

/** The charts of one result, made once for it as the heatmaps are large. */
function Charts({ result, kinematics, name }: { result: VibrationsResult; kinematics: string; name: string }) {
  const theme = useChartTheme();
  const options = useMemo(
    () => ({
      polarAngle: polarAngleOption(result, theme),
      polarHeatmap: polarHeatmapOption(result, theme),
      speed: speedProfileOption(result, theme),
      angular: angularProfilesOption(result, kinematics, theme),
      heatmap: heatmapOption(result, theme),
      motor: result.motor ? motorProfileOption(result.motor, MAX_FREQ, kinematics, theme) : null,
    }),
    [result, kinematics, theme],
  );
  return (
    <>
      <div className="grid gap-4 lg:grid-cols-2">
        <ChartPanel
          title="Angular energy"
          option={options.polarAngle}
          label={`Angular energy of ${name}`}
          height={POLAR_HEIGHT}
          below={<GoodAngleRanges result={result} />}
        />
        <ChartPanel
          title="Polar energy heatmap"
          option={options.polarHeatmap}
          label={`Polar energy heatmap of ${name}`}
          height={POLAR_HEIGHT}
        />
      </div>
      <ChartPanel title="Speed energy profile" option={options.speed} label={`Speed energy profile of ${name}`} />
      <ChartPanel title="Angular speed profiles" option={options.angular} label={`Angular speed profiles of ${name}`} />
      <ChartPanel title="Energy heatmap" option={options.heatmap} label={`Energy heatmap of ${name}`} height={420} />
      {options.motor && result.motor && (
        <ChartPanel
          title="Motor resonance profile"
          option={options.motor}
          label={`Motor resonance profile of ${name}`}
          intro={<MotorFigures motor={result.motor} />}
        />
      )}
    </>
  );
}

function DataQuality({ chips, onInclude }: { chips: ChipVibrations[]; onInclude: () => void }) {
  const problems = chips.filter((c) => c.excluded !== null || c.failure !== null);
  if (problems.length === 0) return null;
  return (
    <Panel title="Data quality">
      <ul className="space-y-1 text-sm">
        {problems.map((c) => (
          <li key={c.chip}>
            <span className="font-mono font-medium">{c.chip}</span>
            {c.failure !== null && (
              <span className="text-destructive">{`: could not be analysed: ${c.failure}. It is not included in All chips.`}</span>
            )}
            {c.failure === null && c.excluded !== null && (
              <span>{`: left out of All chips. ${explainReason(c.excluded)}`}</span>
            )}
          </li>
        ))}
      </ul>
      {problems.some((c) => c.excluded !== null && c.failure === null) && (
        <Button type="button" variant="outline" size="sm" onClick={onInclude}>
          Include excluded chips anyway
        </Button>
      )}
    </Panel>
  );
}

/** The panels of the selected tab: a chip, or null for the chips combined. */
export function VibrationsView({
  perChip,
  combined,
  chip,
  kinematics,
  onInclude,
}: {
  perChip: ChipVibrations[];
  combined: VibrationsResult | null;
  chip: string | null;
  kinematics: string;
  onInclude: () => void;
}) {
  if (chip === null) {
    return (
      <>
        <DataQuality chips={perChip} onInclude={onInclude} />
        {combined ? (
          <Charts result={combined} kinematics={kinematics} name="all chips" />
        ) : (
          <p role="status" className="text-sm">
            No chip qualifies for All chips.
          </p>
        )}
      </>
    );
  }
  const entry = perChip.find((c) => c.chip === chip);
  if (!entry) return null;
  return (
    <>
      <DataQuality chips={[entry]} onInclude={onInclude} />
      {entry.result && <Charts result={entry.result} kinematics={kinematics} name={chipLabel(chip)} />}
    </>
  );
}
