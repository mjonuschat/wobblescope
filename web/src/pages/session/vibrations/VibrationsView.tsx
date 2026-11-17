import { useMemo } from "react";

import { Panel } from "@/components/Panel";
import { EChart } from "@/components/charts/EChart";
import { Button } from "@/components/ui/button";
import { chipLabel } from "@/lib/format";
import type { ChipVibrations } from "@/lib/generated/ChipVibrations";
import type { VibrationsResult } from "@/lib/generated/VibrationsResult";

import { explainReason } from "../EligibilityPanel";
import {
  angularProfilesOption,
  heatmapOption,
  motorProfileOption,
  polarAngleOption,
  polarHeatmapOption,
  speedProfileOption,
} from "./vibrationsOptions";

/** The highest frequency of the motor profile in Hz. The analysis keeps the frequencies up to it. */
export const MAX_FREQ = 1000;

/** The charts of one result, made once for it as the heatmaps are large. */
function Charts({ result, kinematics, name }: { result: VibrationsResult; kinematics: string; name: string }) {
  const options = useMemo(
    () => ({
      polarAngle: polarAngleOption(result),
      polarHeatmap: polarHeatmapOption(result),
      speed: speedProfileOption(result),
      angular: angularProfilesOption(result, kinematics),
      heatmap: heatmapOption(result),
      motor: result.motor ? motorProfileOption(result.motor, MAX_FREQ, kinematics) : null,
    }),
    [result, kinematics],
  );
  return (
    <>
      <div className="grid gap-4 lg:grid-cols-2">
        <Panel title="Angular Energy" label="Angular Energy">
          <EChart option={options.polarAngle} label={`Angular energy of ${name}`} height={380} />
        </Panel>
        <Panel title="Polar Energy Heatmap" label="Polar Energy Heatmap">
          <EChart option={options.polarHeatmap} label={`Polar energy heatmap of ${name}`} height={380} />
        </Panel>
      </div>
      <Panel title="Speed Energy Profile" label="Speed Energy Profile">
        <EChart option={options.speed} label={`Speed energy profile of ${name}`} height={380} zoom />
      </Panel>
      <Panel title="Angular Speed Profiles" label="Angular Speed Profiles">
        <EChart option={options.angular} label={`Angular speed profiles of ${name}`} height={380} zoom />
      </Panel>
      <Panel title="Energy Heatmap" label="Energy Heatmap">
        <EChart option={options.heatmap} label={`Energy heatmap of ${name}`} height={420} />
      </Panel>
      {options.motor && (
        <Panel title="Motor Resonance Profile" label="Motor Resonance Profile">
          <EChart option={options.motor} label={`Motor resonance profile of ${name}`} height={380} zoom />
        </Panel>
      )}
    </>
  );
}

/** What is wrong with a chip's data, with the way to include a chip that was left out. */
function DataQuality({ chips, onInclude }: { chips: ChipVibrations[]; onInclude: () => void }) {
  const problems = chips.filter((c) => c.excluded !== null || c.failure !== null);
  if (problems.length === 0) return null;
  return (
    <Panel title="Data Quality">
      <ul className="space-y-1 text-sm">
        {problems.map((c) => (
          <li key={c.chip}>
            <span className="font-mono font-medium">{c.chip}</span>
            {c.failure !== null && (
              <span className="text-destructive">{`: could not be analysed: ${c.failure}. It is not part of the worst case.`}</span>
            )}
            {c.failure === null && c.excluded !== null && (
              <span>{`: left out of the worst case. ${explainReason(c.excluded)}`}</span>
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

/** The panels of the selected tab: a chip, or null for the worst case. */
export function VibrationsView({
  perChip,
  worstCase,
  chip,
  kinematics,
  onInclude,
}: {
  perChip: ChipVibrations[];
  worstCase: VibrationsResult | null;
  chip: string | null;
  kinematics: string;
  onInclude: () => void;
}) {
  if (chip === null) {
    return (
      <>
        <DataQuality chips={perChip} onInclude={onInclude} />
        {worstCase ? (
          <Charts result={worstCase} kinematics={kinematics} name="the worst case" />
        ) : (
          <p role="status" className="text-sm">
            No chip qualifies for the worst case.
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
