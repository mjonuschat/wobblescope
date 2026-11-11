import { OctagonAlertIcon } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";

import { Panel } from "@/components/Panel";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useSettings, useStopTest } from "@/lib/api/queries";
import { useConfirmClick } from "@/lib/confirm";
import type { HoldAxis } from "@/lib/generated/HoldAxis";
import type { HoldEnded } from "@/lib/generated/HoldEnded";
import type { HoldState } from "@/lib/generated/HoldState";
import { cn } from "@/lib/utils";

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

/** The duration choices. `until` runs until stopped, up to the service's cap. */
const DURATIONS = [
  { value: "10", text: "10 s" },
  { value: "30", text: "30 s" },
  { value: "60", text: "60 s" },
  { value: "120", text: "120 s" },
  { value: "until", text: "Until stopped" },
];

const EMERGENCY_TEXT = "Emergency stop. Klipper is in shutdown and needs a FIRMWARE_RESTART before it moves again.";

const PHASES: Record<HoldState["phase"], string> = {
  travelling: "Moving to the start point",
  holding: "Holding",
  stopping: "Stopping",
};

function endText(ended: HoldEnded): string {
  const { reason } = ended;
  if (typeof reason === "object") return `The hold failed: ${reason.failed.message}`;
  return {
    time_limit: "The time limit was reached.",
    stopped: "The hold was stopped.",
    connection_lost: "The connection to this page was lost, so the hold stopped.",
    emergency_stop: EMERGENCY_TEXT,
  }[reason];
}

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
}

/** Starts, changes and stops a held excitation. */
export function HoldPanel({ hold, frequencyOfInterest }: Props) {
  const settings = useSettings();
  const stopTest = useStopTest();
  const [axis, setAxis] = useState<HoldAxis>("X");
  const [frequency, setFrequency] = useState(String(frequencyOfInterest ?? DEFAULT_HZ));
  const [accel, setAccel] = useState<string | null>(null);
  const [duration, setDuration] = useState("30");
  const [custom, setCustom] = useState(false);
  const [point, setPoint] = useState(["", "", ""]);

  const running = hold.status === "running" || hold.status === "starting";
  const confirm = useConfirmClick(() => hold.id && stopTest.mutate(hold.id));

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
    if (!valid || hz === null || accelValue === null) return;
    hold.start({
      axis,
      frequency: hz,
      accel_per_hz: accelValue,
      duration_seconds: duration === "until" ? null : Number(duration),
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

  const stopLabel = confirm.armed ? "Press again to emergency stop" : "Emergency stop";
  const state = hold.state;

  return (
    <Panel title="Held Excitation">
      <div className="flex flex-wrap items-end gap-4">
        <Row label="Axis" htmlFor="hold-axis">
          <Select value={axis} onValueChange={(v) => changeAxis(v as HoldAxis)}>
            <SelectTrigger id="hold-axis" className="w-32">
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
        <Row label="Frequency (Hz)" htmlFor="hold-frequency">
          <Input
            id="hold-frequency"
            type="number"
            className="w-28"
            value={frequency}
            onChange={(e) => changeFrequency(e.target.value)}
          />
        </Row>
        <Row label="Acceleration per Hz" htmlFor="hold-accel">
          <Input
            id="hold-accel"
            type="number"
            className="w-32"
            value={accelText}
            disabled={running}
            onChange={(e) => setAccel(e.target.value)}
          />
        </Row>
        <Row label="Duration" htmlFor="hold-duration">
          <Select value={duration} onValueChange={(v) => setDuration(String(v))} disabled={running}>
            <SelectTrigger id="hold-duration" className="w-40">
              <SelectValue>{DURATIONS.find((d) => d.value === duration)?.text}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              {DURATIONS.map((d) => (
                <SelectItem key={d.value} value={d.value}>
                  {d.text}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Row>
        <Row label="Start point" htmlFor="hold-point">
          <Select
            value={custom ? "custom" : "test"}
            onValueChange={(v) => setCustom(v === "custom")}
            disabled={running}
          >
            <SelectTrigger id="hold-point" className="w-40">
              <SelectValue>{custom ? "Custom" : "Test point"}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="test">Test point</SelectItem>
              <SelectItem value="custom">Custom</SelectItem>
            </SelectContent>
          </Select>
        </Row>
        {custom &&
          ["X", "Y", "Z"].map((name, i) => (
            <Row key={name} label={`${name} (mm)`} htmlFor={`hold-point-${name}`}>
              <Input
                id={`hold-point-${name}`}
                type="number"
                className="w-24"
                value={point[i]}
                disabled={running}
                onChange={(e) => setPoint(point.map((p, j) => (j === i ? e.target.value : p)))}
              />
            </Row>
          ))}
      </div>
      <div className="flex flex-wrap items-center gap-3">
        {running ? (
          <Button type="button" variant="outline" onClick={() => hold.stop()}>
            Stop
          </Button>
        ) : (
          <Button type="button" disabled={!valid} onClick={start}>
            Start
          </Button>
        )}
        <button
          type="button"
          aria-label={stopLabel}
          aria-pressed={confirm.armed}
          disabled={!running || hold.id === null || stopTest.isPending}
          onClick={confirm.click}
          className={cn(
            "flex h-8 items-center gap-1 rounded-md border px-2 text-sm transition-colors focus-visible:outline-2 disabled:opacity-50",
            confirm.armed ? "border-red-500 bg-red-600 text-white" : "text-red-400 hover:bg-accent",
          )}
        >
          <OctagonAlertIcon className="size-4" />
          {confirm.armed ? "Press again" : "Emergency stop"}
        </button>
        {stopTest.isError && <span className="text-sm text-red-400">{stopTest.error.message}</span>}
      </div>
      {state && running && (
        <p className="font-mono text-sm">
          {PHASES[state.phase]}. {state.elapsed_seconds.toFixed(1)} s elapsed. {state.frequency.toFixed(1)} Hz at{" "}
          {Math.round(state.acceleration)} mm/s², acceleration limit in force {Math.round(state.accel_limit)} mm/s².
        </p>
      )}
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
          <p>{endText(hold.ended)}</p>
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
