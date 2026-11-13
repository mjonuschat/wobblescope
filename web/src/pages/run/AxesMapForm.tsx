import { PlayIcon } from "lucide-react";
import { useState } from "react";

import { homedTone } from "@/components/PrinterControls";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { UnitInput } from "@/components/UnitInput";
import { ApiClientError } from "@/lib/api/client";
import { usePrinterState, useStartAxesMap } from "@/lib/api/queries";
import type { AxesMapRequest } from "@/lib/generated/AxesMapRequest";
import type { CheckFailure } from "@/lib/generated/CheckFailure";
import type { ChipsSnapshot } from "@/lib/generated/ChipsSnapshot";
import type { Mounting } from "@/lib/generated/Mounting";
import { parseNumber } from "@/lib/parse";

type ParameterKey = "z_height" | "speed" | "accel" | "travel_speed";

/** Shake&Tune's defaults and the lowest values the form accepts. */
const PARAMETERS: { key: ParameterKey; label: string; unit: string; fallback: number; min: number | null }[] = [
  { key: "z_height", label: "Z height", unit: "mm", fallback: 20, min: null },
  { key: "speed", label: "Speed", unit: "mm/s", fallback: 80, min: 20 },
  { key: "accel", label: "Acceleration", unit: "mm/s²", fallback: 1500, min: 100 },
  { key: "travel_speed", label: "Travel speed", unit: "mm/s", fallback: 120, min: 20 },
];

interface Props {
  chipList: ChipsSnapshot;
  /** Why a run cannot start now, or `null`. */
  blocked: string | null;
  /** Called with the new run's id after the daemon accepted it. */
  onStarted: (id: string) => void;
}

export function AxesMapForm({ chipList, blocked, onStarted }: Props) {
  const start = useStartAxesMap();
  const printer = usePrinterState(true);
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

  let problem = blocked;
  if (!problem && printer.isPending) problem = "Checking whether the printer is homed…";
  if (!problem && printer.data && homedTone(printer.data.homed_axes) !== "full") {
    const axes = printer.data.homed_axes ?? "";
    const notHomed = ["X", "Y", "Z"].filter((a) => !axes.toLowerCase().includes(a.toLowerCase()));
    problem = `Home the printer first. Not homed: ${notHomed.join(", ")}.`;
  }
  if (!problem && ordered.length === 0) problem = "Choose at least one chip.";
  if (!problem && Object.keys(complaints).length > 0) problem = "Fix the settings that are not valid.";

  const error = start.error;
  const body = error instanceof ApiClientError ? error.body : null;
  const failures: CheckFailure[] = body?.error === "preflight_failed" ? body.failures : [];
  const general = error && failures.length === 0 ? error.message : null;

  function submit(event: React.FormEvent) {
    event.preventDefault();
    if (problem || start.isPending) return;
    // The daemon takes an unset setting as absent, which the generated type spells as `null`.
    const request = {
      chips: ordered.map((chip) => ({ chip, mounting: mounting[chip] ?? "toolhead" })),
    } as AxesMapRequest;
    for (const { key, fallback } of PARAMETERS) {
      const value = values[key];
      if (value !== undefined && value !== fallback) request[key] = value;
    }
    start.mutate(request, { onSuccess: (started) => onStarted(started.id) });
  }

  const parameterField = ({ key, label, unit, fallback }: (typeof PARAMETERS)[number]) => (
    <div key={key} className="space-y-1">
      <Label htmlFor={`axes-map-${key}`}>{label}</Label>
      <UnitInput
        id={`axes-map-${key}`}
        unit={unit}
        placeholder={String(fallback)}
        value={texts[key] ?? String(fallback)}
        aria-invalid={Boolean(complaints[key])}
        aria-describedby={`error-${key}`}
        onChange={(e) => setTexts((old) => ({ ...old, [key]: e.target.value }))}
      />
      {complaints[key] && (
        <p id={`error-${key}`} role="alert" className="text-sm text-destructive">
          {complaints[key]}
        </p>
      )}
    </div>
  );

  return (
    <form onSubmit={submit} className="space-y-4" noValidate>
      <Card>
        <CardHeader>
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">Setup</CardTitle>
        </CardHeader>
        <CardContent className="space-y-5">
          <div className="space-y-2">
            <p className="text-sm font-medium">Chips</p>
            {chipList.chips.length === 0 ? (
              <p className="text-sm">Klipper's config lists no supported accelerometer.</p>
            ) : (
              <ul className="space-y-3">
                {chipList.chips.map((chip, index) => {
                  const unavailable = !isUsable(chip.section);
                  const on = selected.includes(chip.section);
                  return (
                    <li
                      key={chip.section}
                      className="flex items-center gap-4"
                      title={chip.availability.state === "unavailable" ? chip.availability.reason : undefined}
                    >
                      <Checkbox
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
                      <Label htmlFor={`axes-map-chip-${index}`} className="min-w-40 font-mono">
                        {chip.section}
                      </Label>
                      <Select
                        value={mounting[chip.section] ?? "toolhead"}
                        onValueChange={(value) => setMounting((old) => ({ ...old, [chip.section]: value as Mounting }))}
                        disabled={!on}
                      >
                        <SelectTrigger className="w-40" aria-label={`Mounted on ${chip.section}`}>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="toolhead">Toolhead</SelectItem>
                          <SelectItem value="bed">Bed</SelectItem>
                        </SelectContent>
                      </Select>
                    </li>
                  );
                })}
              </ul>
            )}
            <p className="text-sm text-muted-foreground">A chip mounted under the nozzle needs a higher Z height</p>
          </div>
          {parameterField(PARAMETERS[0])}
        </CardContent>
      </Card>

      <details className="rounded-lg border p-4">
        <summary className="cursor-pointer text-sm font-medium">Advanced</summary>
        <div className="mt-4 grid grid-cols-3 gap-4">{PARAMETERS.slice(1).map(parameterField)}</div>
      </details>

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
              <p>The calibration did not start:</p>
              <ul className="list-disc pl-5">
                {failures.map((f) => (
                  <li key={f.check}>{f.message}</li>
                ))}
              </ul>
            </div>
          )}
          {problem && <p className="text-sm text-muted-foreground">{problem}</p>}
        </div>
        <Tooltip>
          <TooltipTrigger render={<span className="ml-auto" />}>
            <Button type="submit" size="lg" className="px-5" disabled={problem !== null || start.isPending}>
              {start.isPending ? (
                "Checking printer…"
              ) : (
                <>
                  <PlayIcon data-icon="inline-start" />
                  Start calibration
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
