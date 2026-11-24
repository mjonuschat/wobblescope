import { RotateCcwIcon } from "lucide-react";
import { useState } from "react";

import { HintLabel } from "@/components/HintLabel";
import { Panel } from "@/ui-kit/Panel";
import { UnitInput } from "@/components/UnitInput";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { FitParams } from "@/lib/generated/FitParams";

import { type FitBaseline, type FitDraft, defaultFitParams, draftFrom, draftToParams } from "./fit";

const FIELDS: Record<keyof FitDraft, { label: string; unit: string; step: number }> = {
  scv: { label: "Square corner velocity", unit: "mm/s", step: 1 },
  damping_ratio: { label: "Damping ratio", unit: "ζ", step: 0.01 },
  max_smoothing: { label: "Maximum corner rounding", unit: "mm", step: 0.01 },
  max_vibrations: { label: "Acceptable vibration", unit: "%", step: 0.1 },
};

function recordedDamping(recorded: FitBaseline["recorded"]): string {
  const byAxis = Object.entries(recorded.dampingByAxis ?? {});
  if (byAxis.length > 0) {
    return `The printer's input shaper uses ${byAxis.map(([axis, value]) => `${axis} ${value}`).join(", ")}. `;
  }
  return recorded.damping !== null ? `The printer's input shaper uses ${recorded.damping}. ` : "";
}

function hintFor(key: keyof FitDraft, recorded: FitBaseline["recorded"]): string {
  switch (key) {
    case "scv":
      return `How fast the toolhead takes a corner. It decides how much the shapers round corners. ${
        recorded.scv ? "From the printer when it was tested." : "Klipper's default. Not recorded for this session."
      }`;
    case "damping_ratio":
      return `How quickly the printer's resonance dies out; lower rings longer. ${recordedDamping(
        recorded,
      )}Empty estimates it from the data.`;
    case "max_smoothing":
      return "Skip shapers that round corners more than this. Empty means no limit.";
    case "max_vibrations":
      return "When even the best shaper leaves more vibration than this, prefer sharper corners. Empty uses the default.";
  }
}

const NOTHING_RECORDED: FitBaseline = {
  params: defaultFitParams,
  recorded: { scv: false, damping: null },
};

interface FieldProps {
  name: keyof FitDraft;
  value: string;
  hint: string;
  invalid: boolean;
  onChange: (value: string) => void;
}

function FitField({ name, value, hint, invalid, onChange }: FieldProps) {
  const { label, unit, step } = FIELDS[name];
  const id = `fit-${name}`;
  return (
    <div className="flex flex-col gap-2.5">
      <HintLabel label={label} hint={hint} />
      <UnitInput
        id={id}
        unit={unit}
        step={step}
        value={value}
        aria-label={label}
        aria-invalid={invalid}
        aria-describedby={`${id}-hint`}
        onChange={(e) => onChange(e.target.value)}
      />
      <span id={`${id}-hint`} className="sr-only">
        {hint}
      </span>
    </div>
  );
}

interface Props {
  /** The values to start from, and which of them the printer recorded. Klipper's defaults when omitted. */
  baseline?: FitBaseline;
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
  const field = (name: keyof FitDraft) => (
    <FitField
      name={name}
      value={draft[name]}
      hint={hintFor(name, baseline.recorded)}
      invalid={invalid === name}
      onChange={(value) => setDraft({ ...draft, [name]: value })}
    />
  );

  return (
    <form
      noValidate
      onSubmit={(event) => {
        event.preventDefault();
        if ("params" in result) onApply(result.params);
      }}
    >
      <Panel
        title="Fit settings"
        action={
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  aria-label="Reset"
                  onClick={() => {
                    setDraft(draftFrom(baseline.params));
                    if (onReset) onReset();
                    else onApply(baseline.params);
                  }}
                />
              }
            >
              <RotateCcwIcon />
            </TooltipTrigger>
            <TooltipContent side="top">
              {baseline.recorded.scv ? "Back to the values the printer recorded" : "Back to Klipper's defaults"}
            </TooltipContent>
          </Tooltip>
        }
      >
        <div className="grid items-end gap-4 sm:grid-cols-[repeat(4,minmax(0,1fr))_auto]">
          {field("scv")}
          {field("damping_ratio")}
          {field("max_smoothing")}
          {field("max_vibrations")}
          <Button type="submit" disabled={invalid !== null}>
            Apply
          </Button>
        </div>
        {invalid && <p className="text-sm text-destructive">Enter a number, or leave the field empty.</p>}
      </Panel>
    </form>
  );
}
