import { PlayIcon } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { UnitInput } from "@/components/UnitInput";
import { ApiClientError } from "@/lib/api/client";
import { useSettings, useStartAxesMap } from "@/lib/api/queries";
import type { AxesMapRequest } from "@/lib/generated/AxesMapRequest";
import type { ChipChoice } from "@/lib/generated/ChipChoice";
import type { CheckFailure } from "@/lib/generated/CheckFailure";
import type { ChipsSnapshot } from "@/lib/generated/ChipsSnapshot";
import type { Mounting } from "@/lib/generated/Mounting";
import { MOUNTINGS } from "@/lib/mounting";
import { formatMm, parseNumber } from "@/lib/parse";

import { ChoiceRow, FieldError, FormCard, ResetButton, usePrinterProblem } from "./FormParts";

type ParameterKey = "z_height" | "speed" | "accel" | "travel_speed";

/** Shake&Tune's defaults and the lowest values the form accepts. */
const PARAMETERS: { key: ParameterKey; label: string; unit: string; fallback: number; min: number | null }[] = [
  { key: "z_height", label: "Z height", unit: "mm", fallback: 20, min: null },
  { key: "speed", label: "Speed", unit: "mm/s", fallback: 80, min: 20 },
  { key: "accel", label: "Acceleration", unit: "mm/s²", fallback: 1500, min: 100 },
  { key: "travel_speed", label: "Travel speed", unit: "mm/s", fallback: 120, min: 20 },
];

/** How far the X and Y moves reach from the middle of the travel. It must match the daemon's `HALF_SPAN` in `capture/axes_map.rs`. */
const HALF_SPAN = 15;

interface Props {
  chipList: ChipsSnapshot;
  /** Why a run cannot start now, or `null`. */
  blocked: string | null;
  /** Called with the new run's id after the daemon accepted it. */
  onStarted: (id: string) => void;
}

export function AxesMapForm({ chipList, blocked, onStarted }: Props) {
  const start = useStartAxesMap();
  const limits = useSettings().data?.axis_limits ?? null;
  const startOf = (axis: "x" | "y") => (limits ? formatMm((limits[axis].min + limits[axis].max) / 2 - HALF_SPAN) : "");
  const isUsable = (section: string) =>
    chipList.chips.find((c) => c.section === section)?.availability.state !== "unavailable";
  const [chosen, setChosen] = useState<string[] | null>(null);
  const [mounting, setMounting] = useState<Record<string, Mounting>>({});
  const [texts, setTexts] = useState<Partial<Record<ParameterKey, string>>>({});

  const selected = (chosen ?? [...new Set([...chipList.defaults.x, ...chipList.defaults.y])]).filter(isUsable);
  const ordered = chipList.chips.map((c) => c.section).filter((section) => selected.includes(section));

  const values: Partial<Record<ParameterKey, number>> = {};
  const complaints: Partial<Record<ParameterKey, string>> = {};
  for (const { key, label, min } of PARAMETERS) {
    const value = parseNumber(texts[key] ?? "");
    if (value === undefined) continue;
    if (Number.isNaN(value)) complaints[key] = "Enter a number.";
    else if (min !== null && value < min)
      complaints[key] = `${label} must be at least ${min} mm/s${key === "accel" ? "²" : ""}.`;
    else if (key === "z_height" && value <= 0) complaints[key] = "Z height must be above 0 mm.";
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
    const draft: { chips: ChipChoice[] } & Partial<Omit<AxesMapRequest, "chips">> = {
      chips: ordered.map((chip) => ({ chip, mounting: mounting[chip] ?? "toolhead" })),
    };
    for (const { key, fallback } of PARAMETERS) {
      const value = values[key];
      if (value !== undefined && value !== fallback) draft[key] = value;
    }
    // The daemon takes an unset setting as absent, which the generated type spells as `null`,
    // and JSON.stringify drops the undefined fields.
    const request = draft as AxesMapRequest;
    start.mutate(request, { onSuccess: (started) => onStarted(started.id) });
  }

  const anyOverride = PARAMETERS.some(({ key }) => (texts[key] ?? "") !== "");

  const parameterField = ({ key, label, unit, fallback }: (typeof PARAMETERS)[number], className?: string) => (
    <div key={key} className="space-y-1">
      <Label htmlFor={`axes-map-${key}`}>{key === "z_height" ? "Z" : label}</Label>
      <UnitInput
        id={`axes-map-${key}`}
        unit={unit}
        className={className}
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
        <div className="space-y-5">
          <div className="space-y-2">
            {chipList.chips.length === 0 ? (
              <p className="text-sm">Klipper's config lists no supported accelerometer.</p>
            ) : (
              <div className="grid grid-cols-[13rem_10rem] items-center gap-x-4 gap-y-3">
                <p className="text-sm font-medium">Chips</p>
                <p className="text-sm font-medium">Mounting</p>
                {chipList.chips.map((chip, index) => {
                  const unavailable = !isUsable(chip.section);
                  const on = selected.includes(chip.section);
                  return (
                    <div key={chip.section} className="contents">
                      <div
                        className="flex items-center gap-2"
                        title={chip.availability.state === "unavailable" ? chip.availability.reason : undefined}
                      >
                        <Switch
                          id={`axes-map-chip-${index}`}
                          checked={on}
                          disabled={unavailable}
                          onCheckedChange={(checked) =>
                            setChosen(
                              chipList.chips
                                .map((c) => c.section)
                                .filter((s) => (s === chip.section ? checked : selected.includes(s))),
                            )
                          }
                        />
                        <Label htmlFor={`axes-map-chip-${index}`} className="font-mono">
                          {chip.section}
                        </Label>
                      </div>
                      <Select
                        value={mounting[chip.section] ?? "toolhead"}
                        onValueChange={(value) => setMounting((old) => ({ ...old, [chip.section]: value as Mounting }))}
                        disabled={!on}
                      >
                        <SelectTrigger className="w-40" aria-label={`Mounted on ${chip.section}`}>
                          <SelectValue>
                            {MOUNTINGS.find((m) => m.value === (mounting[chip.section] ?? "toolhead"))?.text}
                          </SelectValue>
                        </SelectTrigger>
                        <SelectContent>
                          {MOUNTINGS.map((m) => (
                            <SelectItem key={m.value} value={m.value}>
                              {m.text}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
          <ChoiceRow label="Test point">
            <div className="flex items-end gap-2">
              {(["x", "y"] as const).map((axis) => (
                <div key={axis} className="space-y-1">
                  <Label htmlFor={`axes-map-point-${axis}`}>{axis.toUpperCase()}</Label>
                  <UnitInput
                    id={`axes-map-point-${axis}`}
                    unit="mm"
                    className="w-32"
                    readOnly
                    placeholder="unknown"
                    title="Fixed: the middle of the travel, less 15 mm"
                    value={startOf(axis)}
                  />
                </div>
              ))}
              {parameterField(PARAMETERS[0], "w-32")}
            </div>
          </ChoiceRow>
        </div>
      </FormCard>

      <FormCard title="Parameters" action={<ResetButton disabled={!anyOverride} onClick={() => setTexts({})} />}>
        <div className="grid grid-cols-3 gap-4">{PARAMETERS.slice(1).map((p) => parameterField(p))}</div>
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
