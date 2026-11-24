import { PlayIcon } from "lucide-react";
import { useState } from "react";

import { HintLabel } from "@/components/HintLabel";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { UnitInput } from "@/components/UnitInput";
import { ApiClientError } from "@/lib/api/client";
import { useStartVibrations } from "@/lib/api/queries";
import type { CheckFailure } from "@/lib/generated/CheckFailure";
import type { ChipsSnapshot } from "@/lib/generated/ChipsSnapshot";
import type { VibrationsRequest } from "@/lib/generated/VibrationsRequest";
import { parseNumber } from "@/lib/parse";

import {
  ChoiceRow,
  FieldError,
  FormCard,
  HomeButton,
  ResetButton,
  reportStartFailure,
  usePrinterProblem,
} from "./FormParts";
import { RESOLUTIONS } from "./planning";

type ParameterKey = "size" | "z_height" | "min_speed" | "max_speed" | "speed_increment" | "accel" | "travel_speed";

/** Shake&Tune's defaults and the lowest values the form accepts. */
const PARAMETERS: { key: ParameterKey; label: string; unit: string; fallback: number; min: number | null }[] = [
  { key: "size", label: "Size", unit: "mm", fallback: 100, min: 50 },
  { key: "z_height", label: "Z height", unit: "mm", fallback: 20, min: null },
  { key: "min_speed", label: "Start speed", unit: "mm/s", fallback: 2, min: 2 },
  { key: "max_speed", label: "End speed", unit: "mm/s", fallback: 200, min: 10 },
  { key: "speed_increment", label: "Speed step", unit: "mm/s", fallback: 2, min: 1 },
  { key: "accel", label: "Acceleration", unit: "mm/s²", fallback: 3000, min: 100 },
  { key: "travel_speed", label: "Travel speed", unit: "mm/s", fallback: 120, min: 20 },
];

interface Props {
  chipList: ChipsSnapshot;
  /** Why a run cannot start now, or `null`. */
  blocked: string | null;
  onStarted: (id: string) => void;
}

export function VibrationsForm({ chipList, blocked, onStarted }: Props) {
  const start = useStartVibrations();
  const isUsable = (section: string) =>
    chipList.chips.find((c) => c.section === section)?.availability.state !== "unavailable";
  const [chosen, setChosen] = useState<string[] | null>(null);
  const [resolution, setResolution] = useState<(typeof RESOLUTIONS)[number]["value"]>("motors");
  const [texts, setTexts] = useState<Partial<Record<ParameterKey, string>>>({});

  const selected = (chosen ?? [...new Set([...chipList.defaults.x, ...chipList.defaults.y])]).filter(isUsable);
  const ordered = chipList.chips.map((c) => c.section).filter((section) => selected.includes(section));

  const values: Partial<Record<ParameterKey, number>> = {};
  const complaints: Partial<Record<ParameterKey, string>> = {};
  for (const { key, label, unit, min } of PARAMETERS) {
    const value = parseNumber(texts[key] ?? "");
    if (value === undefined) continue;
    if (Number.isNaN(value)) complaints[key] = "Enter a number.";
    else if (min !== null && value < min) complaints[key] = `${label} must be at least ${min} ${unit}.`;
    else if (key === "z_height" && value <= 0) complaints[key] = "Z height must be above 0 mm.";
    else values[key] = value;
  }
  const startSpeed = values.min_speed ?? 2;
  const endSpeed = values.max_speed ?? 200;
  if (complaints.min_speed === undefined && startSpeed >= endSpeed) {
    complaints.min_speed = "Start speed must be below the end speed.";
  }

  let problem = usePrinterProblem(blocked);
  if (!problem && ordered.length === 0) problem = "Choose at least one chip.";
  if (!problem && Object.keys(complaints).length > 0) problem = "Fix the settings that are not valid.";

  const draft: { chips: string[] } & Partial<Omit<VibrationsRequest, "chips">> = { chips: ordered };
  for (const { key, fallback } of PARAMETERS) {
    const value = values[key];
    if (value !== undefined && value !== fallback) draft[key] = value;
  }
  if (resolution !== "motors") draft.angular_step = Number(resolution);
  // The daemon takes an unset setting as absent, which the generated type spells as `null`,
  // and JSON.stringify drops the undefined fields.
  const request = draft as VibrationsRequest;
  const chosenResolution = RESOLUTIONS.find((r) => r.value === resolution) ?? RESOLUTIONS[0];

  const error = start.error;
  const body = error instanceof ApiClientError ? error.body : null;
  const failures: CheckFailure[] = body?.error === "preflight_failed" ? body.failures : [];

  function submit(event: React.FormEvent) {
    event.preventDefault();
    if (problem || start.isPending) return;
    start.mutate(request, { onSuccess: (started) => onStarted(started.id), onError: reportStartFailure });
  }

  const anyOverride = resolution !== "motors" || PARAMETERS.some(({ key }) => (texts[key] ?? "") !== "");

  const inGroup = (...keys: ParameterKey[]) => PARAMETERS.filter((p) => keys.includes(p.key));

  const parameterField = ({ key, label, unit, fallback }: (typeof PARAMETERS)[number], hint?: string) => (
    <div key={key} className="space-y-1">
      {hint ? <HintLabel label={label} hint={hint} /> : <Label htmlFor={`vibrations-${key}`}>{label}</Label>}
      <UnitInput
        id={`vibrations-${key}`}
        aria-label={hint ? label : undefined}
        unit={unit}
        placeholder={String(fallback)}
        value={texts[key] ?? String(fallback)}
        aria-invalid={Boolean(complaints[key])}
        aria-describedby={`error-${key}`}
        onChange={(e) => setTexts((old) => ({ ...old, [key]: e.target.value }))}
      />
      <FieldError name={key} text={complaints[key]} />
    </div>
  );

  return (
    <form onSubmit={submit} className="space-y-4" noValidate>
      <FormCard title="Setup">
        {chipList.chips.length === 0 ? (
          <p className="text-sm">Klipper's config lists no supported accelerometer.</p>
        ) : (
          <ChoiceRow label="Chips">
            <div className="flex flex-wrap gap-x-6 gap-y-3">
              {chipList.chips.map((chip, index) => {
                const unavailable = !isUsable(chip.section);
                return (
                  <div
                    key={chip.section}
                    className="flex items-center gap-2"
                    title={chip.availability.state === "unavailable" ? chip.availability.reason : undefined}
                  >
                    <Switch
                      id={`vibrations-chip-${index}`}
                      checked={selected.includes(chip.section)}
                      disabled={unavailable}
                      onCheckedChange={(checked) =>
                        setChosen(
                          chipList.chips
                            .map((c) => c.section)
                            .filter((s) => (s === chip.section ? checked : selected.includes(s))),
                        )
                      }
                    />
                    <Label htmlFor={`vibrations-chip-${index}`} className="font-mono text-foreground">
                      {chip.section}
                    </Label>
                  </div>
                );
              })}
            </div>
          </ChoiceRow>
        )}
      </FormCard>

      <FormCard
        title="Parameters"
        action={
          <ResetButton
            disabled={!anyOverride}
            onClick={() => {
              setTexts({});
              setResolution("motors");
            }}
          />
        }
      >
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-3">
            {[...inGroup("size", "z_height"), ...inGroup("accel"), ...inGroup("travel_speed")].map((p) =>
              parameterField(
                p,
                p.key === "travel_speed" ? "The speed of the moves between measured moves." : undefined,
              ),
            )}
          </div>
          <ChoiceRow label="Speed range">
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-3">
              {inGroup("min_speed", "max_speed", "speed_increment").map((p) => parameterField(p))}
            </div>
          </ChoiceRow>
          <ChoiceRow label="Angular resolution">
            <Select value={resolution} onValueChange={(v) => setResolution(v as typeof resolution)}>
              <SelectTrigger id="vibrations-resolution" aria-label="Angular resolution" className="w-72">
                <SelectValue>{`${chosenResolution.label} (${chosenResolution.directions} directions)`}</SelectValue>
              </SelectTrigger>
              <SelectContent>
                {RESOLUTIONS.map((r) => (
                  <SelectItem key={r.value} value={r.value}>
                    {`${r.label} (${r.directions} directions)`}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </ChoiceRow>
        </div>
      </FormCard>

      <Separator />

      <div className="flex items-start justify-between gap-4">
        <div className="space-y-2">
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
          {/* The reason Start is disabled shows in its tooltip. This copy is for screen readers. */}
          {problem && <p className="sr-only">{problem}</p>}
        </div>
        <div className="ml-auto flex items-center gap-2">
          <HomeButton />
          <Tooltip>
            <TooltipTrigger render={<span />}>
              <Button type="submit" size="lg" className="px-5" disabled={problem !== null || start.isPending}>
                {start.isPending ? (
                  "Checking printer…"
                ) : (
                  <>
                    <PlayIcon data-icon="inline-start" />
                    Run test
                  </>
                )}
              </Button>
            </TooltipTrigger>
            {problem && <TooltipContent side="top">{problem}</TooltipContent>}
          </Tooltip>
        </div>
      </div>
    </form>
  );
}
