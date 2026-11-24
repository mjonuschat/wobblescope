import type { EChartsCoreOption } from "echarts/core";
import { type ReactNode, useMemo, useState } from "react";

import { EXPANDED_CHART_HEIGHT, ExpandDialog } from "@/components/ExpandDialog";
import { Field, Fields } from "@/ui-kit/Fields";
import { Panel } from "@/ui-kit/Panel";
import { EChart } from "@/components/charts/EChart";
import { WIDE_CHART } from "@/components/charts/size";
import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { chipLabel } from "@/lib/format";
import type { ChipVibrations } from "@/lib/generated/ChipVibrations";
import type { MotorResult } from "@/lib/generated/MotorResult";
import type { VibrationsResult } from "@/lib/generated/VibrationsResult";

import { explainReason } from "../EligibilityPanel";
import { GoodAngleRanges } from "./VibrationsSummary";
import {
  ANGLE_PRESETS,
  defaultPreset,
  LOW_FREQ_WARNING,
  type AnglePreset,
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

/** The heatmap and its colour bar side by side need this much, or the bar covers the 330 degree labels. */
const POLAR_HEATMAP_MIN_WIDTH = 460;

/** The highest frequency of the motor profile in Hz. The analysis keeps the frequencies up to it. */
export const MAX_FREQ = 1000;

function ChartPanel({
  title,
  option,
  label,
  height = 380,
  minWidth,
  intro,
  selector,
  below,
  legendSwitches,
}: {
  title: string;
  option: EChartsCoreOption;
  label: string;
  height?: number;
  /** Keep the chart this wide and scroll it sideways on a narrow screen. */
  minWidth?: number;
  intro?: ReactNode;
  /** A control that changes what the chart draws, shown in the panel header and in the expanded view. */
  selector?: ReactNode;
  /** What the chart's numbers refer to, shown under it and in the expanded view. */
  below?: ReactNode;
  legendSwitches?: string[];
}) {
  return (
    <Panel
      title={title}
      label={title}
      action={
        <div className="flex items-center gap-2">
          {selector}
          <ExpandDialog title={title}>
            {selector}
            {intro}
            <EChart option={option} label={label} height={EXPANDED_CHART_HEIGHT} legendSwitches={legendSwitches} />
            {below}
          </ExpandDialog>
        </div>
      }
    >
      {intro}
      <EChart option={option} label={label} height={height} minWidth={minWidth} legendSwitches={legendSwitches} />
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

function AnglePresetPicker({ value, onChange }: { value: AnglePreset; onChange: (value: AnglePreset) => void }) {
  return (
    <ToggleGroup
      variant="outline"
      size="sm"
      aria-label="Angles"
      value={[value]}
      onValueChange={(next) => {
        if (next.length > 0) onChange(next[0] as AnglePreset);
      }}
    >
      {ANGLE_PRESETS.map((item) => (
        <ToggleGroupItem key={item.value} value={item.value} aria-label={item.label}>
          {item.short}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  );
}

/** The charts of one result, made once for it as the heatmaps are large. */
function Charts({
  result,
  kinematics,
  angularStep,
  name,
}: {
  result: VibrationsResult;
  kinematics: string;
  angularStep: number | null;
  name: string;
}) {
  const theme = useChartTheme();
  const [preset, setPreset] = useState<AnglePreset>(() => defaultPreset(angularStep));
  const hasStep = angularStep !== null;
  const options = useMemo(
    () => ({
      polarAngle: polarAngleOption(result, theme),
      polarHeatmap: polarHeatmapOption(result, theme),
      speed: speedProfileOption(result, theme),
      angular: angularProfilesOption(result, kinematics, theme, preset, hasStep),
      heatmap: heatmapOption(result, theme),
      motor: result.motor ? motorProfileOption(result.motor, MAX_FREQ, kinematics, theme) : null,
    }),
    [result, kinematics, theme, preset, hasStep],
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
          minWidth={POLAR_HEATMAP_MIN_WIDTH}
          title="Polar energy heatmap"
          option={options.polarHeatmap}
          label={`Polar energy heatmap of ${name}`}
          height={POLAR_HEIGHT}
        />
      </div>
      <ChartPanel
        minWidth={WIDE_CHART}
        title="Speed energy profile"
        option={options.speed}
        label={`Speed energy profile of ${name}`}
      />
      <ChartPanel
        minWidth={WIDE_CHART}
        title="Angular speed profiles"
        option={options.angular}
        label={`Angular speed profiles of ${name}`}
        selector={<AnglePresetPicker value={preset} onChange={setPreset} />}
        legendSwitches={["Major", "Minor"]}
      />
      <ChartPanel
        minWidth={WIDE_CHART}
        title="Energy heatmap"
        option={options.heatmap}
        label={`Energy heatmap of ${name}`}
        height={420}
      />
      {options.motor && result.motor && (
        <ChartPanel
          minWidth={WIDE_CHART}
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
  angularStep,
  onInclude,
}: {
  perChip: ChipVibrations[];
  combined: VibrationsResult | null;
  chip: string | null;
  kinematics: string;
  angularStep: number | null;
  onInclude: () => void;
}) {
  if (chip === null) {
    return (
      <>
        <DataQuality chips={perChip} onInclude={onInclude} />
        {combined ? (
          <Charts result={combined} kinematics={kinematics} angularStep={angularStep} name="all chips" />
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
      {entry.result && (
        <Charts result={entry.result} kinematics={kinematics} angularStep={angularStep} name={chipLabel(chip)} />
      )}
    </>
  );
}
