import { PlayIcon, SquareIcon } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";

import { Panel } from "@/components/Panel";
import { UnitInput } from "@/components/UnitInput";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useSettings } from "@/lib/api/queries";
import type { HoldAxis } from "@/lib/generated/HoldAxis";
import type { HoldState } from "@/lib/generated/HoldState";

import { PAUSE_COLOURS, RESUME_COLOURS } from "./DisplaySettings";
import type { HoldApi } from "./useHold";

const MIN_HZ = 5;
const MAX_HZ = 200;
const DEFAULT_HZ = 25;

const AXES: { value: HoldAxis; text: string }[] = [
  { value: "X", text: "X" },
  { value: "Y", text: "Y" },
  { value: "A", text: "A (X+Y)" },
  { value: "B", text: "B (X-Y)" },
];

const PHASES: Partial<Record<HoldState["phase"], string>> = {
  travelling: "Moving to the start point.",
  stopping: "Stopping.",
};

function Row({ label, htmlFor, children }: { label: string; htmlFor: string; children: ReactNode }) {
  return (
    <div className="space-y-2">
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
    </div>
  );
}

function parseNumber(text: string): number | null {
  if (text.trim() === "") return null;
  const value = Number(text);
  return Number.isFinite(value) ? value : null;
}

interface Props {
  hold: HoldApi;
  /** The frequency picked on the spectrum, which the form starts from. */
  frequencyOfInterest: number | null;
  /** Whether a chip is streaming. A hold is watched through one, so it will not start without. */
  streaming: boolean;
}

/** Starts, changes and stops a held excitation. */
export function HoldPanel({ hold, frequencyOfInterest, streaming }: Props) {
  const settings = useSettings();
  const [axis, setAxis] = useState<HoldAxis>("X");
  const [frequency, setFrequency] = useState(String(frequencyOfInterest ?? DEFAULT_HZ));
  const [accel, setAccel] = useState<string | null>(null);
  const [custom, setCustom] = useState(false);
  const [point, setPoint] = useState(["", "", ""]);

  const running = hold.status === "running" || hold.status === "starting";

  // A frequency picked on the spectrum is what the form offers next, unless a hold is running.
  useEffect(() => {
    if (frequencyOfInterest !== null && !running) setFrequency(String(frequencyOfInterest));
  }, [frequencyOfInterest, running]);

  const defaultAccel = settings.data?.settings.accel_per_hz.value ?? null;
  const accelText = accel ?? (defaultAccel === null ? "" : String(defaultAccel));
  const hz = parseNumber(frequency);
  const accelValue = parseNumber(accelText);
  const pointValues = point.map(parseNumber);
  const valid =
    hz !== null &&
    hz >= MIN_HZ &&
    hz <= MAX_HZ &&
    accelValue !== null &&
    accelValue > 0 &&
    (!custom || pointValues.every((v) => v !== null));

  function start() {
    if (!valid || !streaming || hz === null || accelValue === null) return;
    hold.start({
      axis,
      frequency: hz,
      accel_per_hz: accelValue,
      duration_seconds: null,
      start_point: custom ? (pointValues as [number, number, number]) : null,
    });
  }

  function changeFrequency(text: string) {
    setFrequency(text);
    const value = parseNumber(text);
    if (running && value !== null && value >= MIN_HZ && value <= MAX_HZ) hold.set({ frequency: value });
  }

  function changeAxis(next: HoldAxis) {
    setAxis(next);
    if (running) hold.set({ axis: next });
  }

  const state = hold.state;

  return (
    <Panel
      title="Excitation"
      action={
        <div className="flex items-center gap-2">
          {running ? (
            <Button type="button" variant="outline" size="sm" className={PAUSE_COLOURS} onClick={() => hold.stop()}>
              <SquareIcon />
              Stop
            </Button>
          ) : (
            <Button
              type="button"
              variant="outline"
              size="sm"
              className={RESUME_COLOURS}
              disabled={!valid || !streaming}
              onClick={start}
            >
              <PlayIcon />
              Start
            </Button>
          )}
        </div>
      }
    >
      <div className="flex flex-wrap items-start gap-4">
        <Row label="Axis" htmlFor="hold-axis">
          <Select value={axis} onValueChange={(v) => changeAxis(v as HoldAxis)}>
            <SelectTrigger id="hold-axis" className="w-40">
              <SelectValue>{AXES.find((a) => a.value === axis)?.text}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              {AXES.map((a) => (
                <SelectItem key={a.value} value={a.value}>
                  {a.text}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Row>
        <Row label="Frequency" htmlFor="hold-frequency">
          <UnitInput
            id="hold-frequency"
            unit="Hz"
            className="w-40"
            value={frequency}
            onChange={(e) => changeFrequency(e.target.value)}
          />
        </Row>
        <Row label="Acceleration per Hz" htmlFor="hold-accel">
          <UnitInput
            id="hold-accel"
            unit="mm/s² per Hz"
            className="w-52"
            value={accelText}
            disabled={running}
            onChange={(e) => setAccel(e.target.value)}
          />
        </Row>
        <Row label="Start Point" htmlFor="hold-point">
          <Select
            value={custom ? "custom" : "test"}
            onValueChange={(v) => setCustom(v === "custom")}
            disabled={running}
          >
            <SelectTrigger id="hold-point" className="w-40">
              <SelectValue>{custom ? "Custom" : "Default"}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="test">Default</SelectItem>
              <SelectItem value="custom">Custom</SelectItem>
            </SelectContent>
          </Select>
        </Row>
      </div>
      {custom && (
        <div className="flex flex-wrap items-start gap-4">
          {["X", "Y", "Z"].map((name, i) => (
            <Row key={name} label={name} htmlFor={`hold-point-${name}`}>
              <UnitInput
                id={`hold-point-${name}`}
                unit="mm"
                className="w-40"
                value={point[i]}
                disabled={running}
                onChange={(e) => setPoint(point.map((p, j) => (j === i ? e.target.value : p)))}
              />
            </Row>
          ))}
        </div>
      )}
      {state && running && PHASES[state.phase] && <p className="text-sm">{PHASES[state.phase]}</p>}
      {hold.rejected && <p className="text-sm text-amber-400">{hold.rejected}</p>}
      {hold.failures.length > 0 && (
        <ul className="list-disc space-y-1 pl-5 text-sm text-amber-400">
          {hold.failures.map((f) => (
            <li key={f.check}>
              {f.message}
              {f.check === "homed" && ". Use the Home button in the page header."}
            </li>
          ))}
        </ul>
      )}
      {hold.error && <p className="text-sm text-red-400">{hold.error}</p>}
      {hold.ended && (
        <div className="space-y-2 text-sm">
          {typeof hold.ended.reason === "object" && <p>The hold failed: {hold.ended.reason.failed.message}</p>}
          {typeof hold.ended.restore === "object" && (
            <div className="space-y-2 text-red-400">
              <p>The printer's settings could not be put back. Still changed:</p>
              <ul className="list-disc pl-5">
                {hold.ended.restore.failed.left_changed.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
              <p>Run this in the console to fix it:</p>
              <pre className="select-all whitespace-pre-wrap rounded border p-2 font-mono text-xs">
                {hold.ended.restore.failed.fix}
              </pre>
            </div>
          )}
        </div>
      )}
    </Panel>
  );
}
