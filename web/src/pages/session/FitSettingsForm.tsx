import { useState } from "react";

import { Panel } from "@/components/Panel";
import { UnitInput } from "@/components/UnitInput";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import type { FitParams } from "@/lib/generated/FitParams";

import { type FitBaseline, type FitDraft, defaultFitParams, draftFrom, draftToParams } from "./fit";

const FIELDS: { key: keyof FitDraft; label: string; unit: string }[] = [
  { key: "scv", label: "Square corner velocity", unit: "mm/s" },
  { key: "damping_ratio", label: "Damping ratio", unit: "" },
  { key: "max_smoothing", label: "Smoothing limit", unit: "mm" },
  { key: "max_vibrations", label: "Vibration limit", unit: "fraction" },
];

/** What sits under a field: where its starting value came from, or what an empty field means. */
function hintFor(key: keyof FitDraft, recorded: FitBaseline["recorded"]): string {
  switch (key) {
    case "scv":
      return recorded.scv
        ? "From the printer when it was tested."
        : "Klipper's default. Not recorded for this session.";
    case "damping_ratio":
      return recorded.damping ? "From the printer's input shaper settings." : "Empty uses 0.1.";
    case "max_smoothing":
      return "Empty means no limit.";
    case "max_vibrations":
      return "Empty uses the default.";
  }
}

const NOTHING_RECORDED: FitBaseline = {
  params: defaultFitParams,
  recorded: { scv: false, damping: false },
};

interface Props {
  /** The values to start from, and which of them the printer recorded. Klipper's defaults when omitted. */
  baseline?: FitBaseline;
  /** The settings the view is using now, when the user changed them. */
  applied?: FitParams | null;
  onApply: (params: FitParams) => void;
  /** Called by Reset. Without it, Reset applies the baseline. */
  onReset?: () => void;
}

/** Fit settings for this view only; nothing here is saved. */
export function FitSettingsForm({ baseline = NOTHING_RECORDED, applied = null, onApply, onReset }: Props) {
  const [draft, setDraft] = useState<FitDraft>(() => draftFrom(applied ?? baseline.params));
  const result = draftToParams(draft);
  const invalid = "invalid" in result ? result.invalid : null;

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if ("params" in result) onApply(result.params);
      }}
    >
      <Panel title="Fit settings">
        <div className="grid gap-4 sm:grid-cols-4">
          {FIELDS.map(({ key, label, unit }) => (
            <div key={key} className="space-y-1">
              <Label htmlFor={`fit-${key}`}>{label}</Label>
              <UnitInput
                id={`fit-${key}`}
                unit={unit}
                step={key === "scv" ? 1 : 0.01}
                value={draft[key]}
                aria-invalid={invalid === key}
                onChange={(e) => setDraft({ ...draft, [key]: e.target.value })}
              />
              <p className="text-xs text-muted-foreground">{hintFor(key, baseline.recorded)}</p>
            </div>
          ))}
        </div>
        {invalid && <p className="text-sm text-destructive">Enter a number, or leave the field empty.</p>}
        <div className="flex gap-2">
          <Button type="submit" disabled={invalid !== null}>
            Apply
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              setDraft(draftFrom(baseline.params));
              if (onReset) onReset();
              else onApply(baseline.params);
            }}
          >
            Reset
          </Button>
        </div>
      </Panel>
    </form>
  );
}
