import { PlayIcon } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
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

import { FieldError, FormCard, ResetButton, usePrinterProblem } from "./FormParts";

type ParameterKey = "size" | "z_height" | "max_speed" | "speed_increment" | "accel" | "travel_speed";

/** Shake&Tune's defaults and the lowest values the form accepts. */
const PARAMETERS: { key: ParameterKey; label: string; unit: string; fallback: number; min: number | null }[] = [
  { key: "size", label: "Size", unit: "mm", fallback: 100, min: 50 },
  { key: "z_height", label: "Z Height", unit: "mm", fallback: 20, min: null },
  { key: "max_speed", label: "Max Speed", unit: "mm/s", fallback: 200, min: 10 },
  { key: "speed_increment", label: "Speed Increment", unit: "mm/s", fallback: 2, min: 1 },
  { key: "accel", label: "Acceleration", unit: "mm/s²", fallback: 3000, min: 100 },
  { key: "travel_speed", label: "Travel Speed", unit: "mm/s", fallback: 120, min: 20 },
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
    else if (key === "z_height" && value <= 0) complaints[key] = "Z Height must be above 0 mm.";
    else values[key] = value;
  }

  let problem = usePrinterProblem(blocked);
  if (!problem && ordered.length === 0) problem = "Choose at least one chip.";
  if (!problem && Object.keys(complaints).length > 0) problem = "Fix the settings that are not valid.";

  const error = start.error;
  const body = error instanceof ApiClientError ? error.body : null;
  const failures: CheckFailure[] = body?.error === "preflight_failed" ? body.failures : [];
  const general = error && failures.length === 0 ? error.message : null;

  function submit(event: React.FormEvent) {
    event.preventDefault();
    if (problem || start.isPending) return;
    const draft: { chips: string[] } & Partial<Omit<VibrationsRequest, "chips">> = { chips: ordered };
    for (const { key, fallback } of PARAMETERS) {
      const value = values[key];
      if (value !== undefined && value !== fallback) draft[key] = value;
    }
    // The daemon takes an unset setting as absent, which the generated type spells as `null`,
    // and JSON.stringify drops the undefined fields.
    const request = draft as VibrationsRequest;
    start.mutate(request, { onSuccess: (started) => onStarted(started.id) });
  }

  const anyOverride = PARAMETERS.some(({ key }) => (texts[key] ?? "") !== "");

  const parameterField = ({ key, label, unit, fallback }: (typeof PARAMETERS)[number]) => (
    <div key={key} className="space-y-1">
      <Label htmlFor={`vibrations-${key}`}>{label}</Label>
      <UnitInput
        id={`vibrations-${key}`}
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
          <div className="space-y-2">
            <p className="text-sm font-medium">Chips</p>
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
                  <Label htmlFor={`vibrations-chip-${index}`} className="font-mono">
                    {chip.section}
                  </Label>
                </div>
              );
            })}
          </div>
        )}
      </FormCard>

      <FormCard title="Parameters" action={<ResetButton disabled={!anyOverride} onClick={() => setTexts({})} />}>
        <div className="grid grid-cols-3 gap-4">{PARAMETERS.map((p) => parameterField(p))}</div>
      </FormCard>

      <Separator />

      <div className="flex items-start justify-between gap-4">
        <div className="space-y-2">
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
          {/* The reason Start is disabled shows in its tooltip. This copy is for screen readers. */}
          {problem && <p className="sr-only">{problem}</p>}
        </div>
        <Tooltip>
          <TooltipTrigger render={<span className="ml-auto" />}>
            <Button type="submit" size="lg" className="px-5" disabled={problem !== null || start.isPending}>
              {start.isPending ? (
                "Checking printer…"
              ) : (
                <>
                  <PlayIcon data-icon="inline-start" />
                  Run Test
                </>
              )}
            </Button>
          </TooltipTrigger>
          {problem && <TooltipContent side="top">{problem}</TooltipContent>}
        </Tooltip>
      </div>
    </form>
  );
}
