import { MinusIcon, PlusIcon } from "lucide-react";
import { type ReactNode, useState } from "react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { UnitInput } from "@/components/UnitInput";
import { ApiClientError } from "@/lib/api/client";
import { useStartTest } from "@/lib/api/queries";
import type { Axis } from "@/lib/generated/Axis";
import type { CheckFailure } from "@/lib/generated/CheckFailure";
import type { ChipsSnapshot } from "@/lib/generated/ChipsSnapshot";
import type { Mode } from "@/lib/generated/Mode";
import type { PrinterSettings } from "@/lib/generated/PrinterSettings";
import { parseNumber } from "@/lib/parse";

import { OVERRIDE_FIELDS, type OverrideKey, formatDuration, segmentSeconds } from "./planning";

/** The consequence of the emergency stop, shown before a test starts. */
export const STOP_CONSEQUENCE =
  "Emergency stop puts Klipper into shutdown. You will need a FIRMWARE_RESTART afterwards.";

/** The settings only one mode uses; they sit beside the mode choice. */
const isModeField = (key: OverrideKey) => key === "sweeping_accel" || key === "sweeping_period";

/** The limits the daemon accepts for a sweep's start and end, in Hz. */
const FREQ_MIN = 1;
const FREQ_MAX = 300;

/** How far the arrows of a number field move it; the others move by 1. */
const FIELD_STEPS: Partial<Record<OverrideKey, number>> = { hz_per_sec: 0.1, sweeping_period: 0.1 };

const FREQ_KEYS = ["freq_start", "freq_end"] as const;

const AXIS_ORDER: Axis[] = ["X", "Y"];

interface PointRow {
  x: string;
  y: string;
  z: string;
}

interface Props {
  settings: PrinterSettings;
  /** Every configured chip. */
  chipList: ChipsSnapshot;
  /** The chips that will record. */
  chips: string[];
  onChipsChange: (chips: string[]) => void;
  /** Why a test cannot start now, or `null`. */
  blocked: string | null;
  /** Called with the new test's id after the daemon accepted the test. */
  onStarted: (id: string) => void;
}

function rowFrom(point: [number, number, number] | null): PointRow {
  return point ? { x: String(point[0]), y: String(point[1]), z: String(point[2]) } : { x: "", y: "", z: "" };
}

/** The daemon names the field it refuses as a `field: reason` prefix. */
function fieldOf(message: string): { field: string; text: string } | null {
  const match = /^([a-z_]+): (.*)$/s.exec(message);
  return match ? { field: match[1], text: match[2] } : null;
}

function FieldError({ name, text }: { name: string; text: string | undefined }) {
  if (!text) return null;
  return (
    <p id={`error-${name}`} role="alert" className="text-sm text-destructive">
      {text}
    </p>
  );
}

function FormCard({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{title}</CardTitle>
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}

function ChoiceRow({ label, error, children }: { label: string; error: ReactNode; children: ReactNode }) {
  return (
    <div className="space-y-2">
      <p className="text-sm font-medium">{label}</p>
      {children}
      {error}
    </div>
  );
}

/** The run form: chips, axes, mode, settings, points, and the start button. */
export function TestForm({ settings, chipList, chips, onChipsChange, blocked, onStarted }: Props) {
  const start = useStartTest();
  const [axes, setAxes] = useState<Axis[]>(["X", "Y"]);
  const [chosenMode, setChosenMode] = useState<Mode | null>(null);
  const [texts, setTexts] = useState<Partial<Record<OverrideKey, string>>>({});
  const [points, setPoints] = useState<PointRow[]>(() => [rowFrom(settings.default_point)]);
  const mode = chosenMode ?? settings.default_mode;

  const overrides: Partial<Record<OverrideKey, number>> = {};
  const typos: Partial<Record<OverrideKey, string>> = {};
  for (const { key } of OVERRIDE_FIELDS) {
    const value = parseNumber(texts[key] ?? "");
    if (value === undefined) continue;
    if (Number.isNaN(value)) typos[key] = "Enter a number.";
    else overrides[key] = value;
  }
  const sweepOnly = (key: OverrideKey) => mode === "Pulse" && (key === "sweeping_accel" || key === "sweeping_period");

  const parsedPoints = points.map((p) => [parseNumber(p.x), parseNumber(p.y), parseNumber(p.z)]);
  const pointsValid =
    parsedPoints.length > 0 && parsedPoints.every((p) => p.every((v) => v !== undefined && !Number.isNaN(v)));
  const missing = OVERRIDE_FIELDS.filter(
    ({ key }) => settings.settings[key].needed && settings.settings[key].value === null && overrides[key] === undefined,
  );

  const seconds = segmentSeconds(settings, overrides);
  const segmentCount = points.length * axes.length;

  let problem = blocked;
  if (!problem && chips.length === 0) problem = "Choose at least one chip.";
  if (!problem && axes.length === 0) problem = "Choose at least one axis.";
  if (!problem && !pointsValid) problem = "Give every test point an X, Y and Z.";
  if (!problem && missing.length > 0) {
    problem = `Set ${missing.map((m) => m.label.toLowerCase()).join(", ")}: the printer does not report it.`;
  }
  if (!problem && Object.keys(typos).length > 0) problem = "Fix the settings that are not numbers.";

  const error = start.error;
  const body = error instanceof ApiClientError ? error.body : null;
  const named = body?.error === "invalid_request" ? fieldOf(body.message) : null;
  const failures: CheckFailure[] = body?.error === "preflight_failed" ? body.failures : [];
  const general =
    error && !named && failures.length === 0
      ? error.message
      : named && !["chips", "axes", "points", ...OVERRIDE_FIELDS.map((f) => f.key)].includes(named.field)
        ? `${named.field}: ${named.text}`
        : null;
  const errorFor = (field: string) => (named?.field === field ? named.text : undefined);

  function submit(event: React.FormEvent) {
    event.preventDefault();
    if (problem || start.isPending) return;
    start.mutate(
      {
        chips,
        axes,
        mode,
        overrides: Object.fromEntries(
          Object.entries(overrides).filter(
            ([key, value]) => !sweepOnly(key as OverrideKey) && value !== settings.settings[key as OverrideKey].value,
          ),
        ),
        points: parsedPoints.map((p) => p as [number, number, number]),
      },
      { onSuccess: (started) => onStarted(started.id) },
    );
  }

  const clampFreq = (value: number) => Math.min(FREQ_MAX, Math.max(FREQ_MIN, value));
  const rangeStart = clampFreq(overrides.freq_start ?? settings.settings.freq_start.value ?? FREQ_MIN);
  const rangeEnd = clampFreq(overrides.freq_end ?? settings.settings.freq_end.value ?? FREQ_MAX);

  const overrideInput = (key: OverrideKey, unit: string, extra: { label?: string; className?: string } = {}) => {
    const setting = settings.settings[key];
    const required = setting.needed && setting.value === null;
    const placeholder = setting.value !== null ? String(setting.value) : required ? "required" : "printer default";
    return (
      <UnitInput
        id={`override-${key}`}
        unit={unit}
        step={FIELD_STEPS[key] ?? 1}
        aria-label={extra.label}
        className={extra.className}
        placeholder={placeholder}
        value={texts[key] ?? (setting.value !== null ? String(setting.value) : "")}
        disabled={!setting.overridable}
        aria-required={required}
        aria-invalid={Boolean(typos[key] ?? errorFor(key))}
        aria-describedby={`error-${key}`}
        onChange={(e) => setTexts((old) => ({ ...old, [key]: e.target.value }))}
      />
    );
  };

  const overrideField = ({ key, label, unit }: (typeof OVERRIDE_FIELDS)[number]) => {
    const setting = settings.settings[key];
    const required = setting.needed && setting.value === null;
    return (
      <div key={key} className="space-y-1">
        <Label htmlFor={`override-${key}`}>
          {label}
          {required ? " *" : ""}
        </Label>
        {overrideInput(key, unit)}
        <FieldError name={key} text={typos[key] ?? errorFor(key)} />
      </div>
    );
  };

  return (
    <form onSubmit={submit} className="space-y-4" noValidate>
      <FormCard title="Setup">
        <div className="space-y-5">
          <ChoiceRow label="Chips" error={<FieldError name="chips" text={errorFor("chips")} />}>
            {chipList.chips.length === 0 ? (
              <p className="text-sm">Klipper's config lists no supported accelerometer.</p>
            ) : (
              <div className="flex flex-wrap gap-x-6 gap-y-3">
                {chipList.chips.map((chip, index) => {
                  const unavailable = chip.availability.state === "unavailable";
                  return (
                    <div
                      key={chip.section}
                      className="flex items-center gap-2"
                      title={chip.availability.state === "unavailable" ? chip.availability.reason : undefined}
                    >
                      <Switch
                        id={`chip-${index}`}
                        checked={chips.includes(chip.section)}
                        disabled={unavailable}
                        onCheckedChange={(on) =>
                          onChipsChange(
                            on
                              ? chipList.chips
                                  .map((c) => c.section)
                                  .filter((n) => n === chip.section || chips.includes(n))
                              : chips.filter((n) => n !== chip.section),
                          )
                        }
                      />
                      <Label htmlFor={`chip-${index}`} className="font-mono">
                        {chip.section}
                      </Label>
                    </div>
                  );
                })}
              </div>
            )}
          </ChoiceRow>
          <ChoiceRow label="Axes" error={<FieldError name="axes" text={errorFor("axes")} />}>
            <div className="flex gap-6">
              {AXIS_ORDER.map((axis) => (
                <div key={axis} className="flex items-center gap-2">
                  <Switch
                    id={`axis-${axis}`}
                    checked={axes.includes(axis)}
                    onCheckedChange={(on) =>
                      setAxes((old) =>
                        on ? AXIS_ORDER.filter((a) => a === axis || old.includes(a)) : old.filter((a) => a !== axis),
                      )
                    }
                  />
                  <Label htmlFor={`axis-${axis}`}>{axis}</Label>
                </div>
              ))}
            </div>
          </ChoiceRow>
          <div className="grid grid-cols-3 items-start gap-4">
            <div className="space-y-2">
              <Label htmlFor="mode">Mode</Label>
              <Select value={mode} onValueChange={(value) => setChosenMode(value as Mode)}>
                <SelectTrigger id="mode" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="Pulse">Pulse</SelectItem>
                  <SelectItem value="Sweep">Sweep</SelectItem>
                </SelectContent>
              </Select>
              <FieldError name="mode" text={errorFor("mode")} />
            </div>
            {OVERRIDE_FIELDS.filter(({ key }) => isModeField(key) && !sweepOnly(key)).map(overrideField)}
          </div>
        </div>
      </FormCard>

      <FormCard title="Parameters">
        <p className="mb-4 text-sm text-muted-foreground">
          The fields start with the printer's values. Change one to override it; clear it to use the printer's value
          again.
        </p>
        <div className="grid grid-cols-2 gap-4">
          <div className="col-span-2 space-y-2">
            <p className="text-sm font-medium">Frequency range</p>
            <div className="flex items-center gap-4">
              {overrideInput("freq_start", "Hz", { label: "Start frequency", className: "w-28 shrink-0" })}
              <Slider
                min={FREQ_MIN}
                max={FREQ_MAX}
                step={1}
                value={[Math.min(rangeStart, rangeEnd), Math.max(rangeStart, rangeEnd)]}
                thumbLabels={["Range start", "Range end"]}
                onValueChange={(next) => {
                  const [low, high] = next as number[];
                  if (low !== rangeStart) setTexts((old) => ({ ...old, freq_start: String(low) }));
                  if (high !== rangeEnd) setTexts((old) => ({ ...old, freq_end: String(high) }));
                }}
              />
              {overrideInput("freq_end", "Hz", { label: "End frequency", className: "w-28 shrink-0" })}
            </div>
            <FieldError name="freq_start" text={typos.freq_start ?? errorFor("freq_start")} />
            <FieldError name="freq_end" text={typos.freq_end ?? errorFor("freq_end")} />
          </div>
          {OVERRIDE_FIELDS.filter(
            ({ key }) => !isModeField(key) && !(FREQ_KEYS as readonly string[]).includes(key),
          ).map(overrideField)}
        </div>
      </FormCard>

      <FormCard title="Points">
        <div className="space-y-2">
          {points.map((row, i) => {
            return (
              // biome-ignore lint/suspicious/noArrayIndexKey: rows have no id and are edited by position
              <div key={i} className="flex items-end gap-2">
                {(["x", "y", "z"] as const).map((axis) => (
                  <div key={axis} className="space-y-1">
                    <Label htmlFor={`point-${i}-${axis}`} className={i === 0 ? undefined : "sr-only"}>
                      {axis.toUpperCase()}
                    </Label>
                    <UnitInput
                      id={`point-${i}-${axis}`}
                      aria-label={`Point ${i + 1} ${axis.toUpperCase()}`}
                      unit="mm"
                      step={1}
                      className="w-32"
                      value={row[axis]}
                      onChange={(e) =>
                        setPoints((old) => old.map((r, j) => (j === i ? { ...r, [axis]: e.target.value } : r)))
                      }
                    />
                  </div>
                ))}
                <div className="flex gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    size="icon"
                    aria-label={`Remove point ${i + 1}`}
                    className={points.length === 1 ? "invisible" : undefined}
                    onClick={() => setPoints((old) => old.filter((_, j) => j !== i))}
                  >
                    <MinusIcon />
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    size="icon"
                    aria-label={`Add a point after point ${i + 1}`}
                    onClick={() =>
                      setPoints((old) => [...old.slice(0, i + 1), rowFrom(settings.default_point), ...old.slice(i + 1)])
                    }
                  >
                    <PlusIcon />
                  </Button>
                </div>
              </div>
            );
          })}
          <FieldError name="points" text={errorFor("points")} />
        </div>
      </FormCard>

      <div className="space-y-2">
        <p className="text-sm">
          {segmentCount} {segmentCount === 1 ? "segment" : "segments"}
          {seconds !== null && segmentCount > 0 ? ` · about ${formatDuration(seconds * segmentCount)}` : ""}
        </p>
        <p className="text-sm text-muted-foreground">{STOP_CONSEQUENCE}</p>
        {general && (
          <p role="alert" className="text-sm text-destructive">
            {general}
          </p>
        )}
        {failures.length > 0 && (
          <div role="alert" className="space-y-1 text-sm text-destructive">
            <p>The test did not start:</p>
            <ul className="list-disc pl-5">
              {failures.map((f) => (
                <li key={f.check}>{f.message}</li>
              ))}
            </ul>
          </div>
        )}
        {problem && <p className="text-sm text-muted-foreground">{problem}</p>}
        <Button type="submit" disabled={problem !== null || start.isPending}>
          {start.isPending ? "Checking printer…" : "Start test"}
        </Button>
      </div>
    </form>
  );
}
