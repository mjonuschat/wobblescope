import { ChevronDownIcon, InfoIcon } from "lucide-react";
import { useState } from "react";

import { Panel } from "@/components/Panel";
import { UnitInput } from "@/components/UnitInput";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Label } from "@/components/ui/label";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { FitParams } from "@/lib/generated/FitParams";

import { type FitBaseline, type FitDraft, defaultFitParams, draftFrom, draftToParams } from "./fit";

const FIELDS: Record<keyof FitDraft, { label: string; unit: string; step: number }> = {
  scv: { label: "Square Corner Velocity", unit: "mm/s", step: 1 },
  damping_ratio: { label: "Damping Ratio", unit: "", step: 0.01 },
  max_smoothing: { label: "Maximum Corner Rounding", unit: "mm", step: 0.01 },
  max_vibrations: { label: "Acceptable Vibration", unit: "fraction", step: 0.01 },
};

/** What a field means, in a sentence or two, including where its starting value came from. */
function hintFor(key: keyof FitDraft, recorded: FitBaseline["recorded"]): string {
  switch (key) {
    case "scv":
      return `How fast the toolhead takes a corner. It decides how much the shapers round corners. ${
        recorded.scv ? "From the printer when it was tested." : "Klipper's default. Not recorded for this session."
      }`;
    case "damping_ratio":
      return `How quickly the printer's resonance dies out; lower rings longer. ${
        recorded.damping ? "From the printer's input shaper settings." : "Empty uses 0.1."
      }`;
    case "max_smoothing":
      return "Skip shapers that round corners more than this. Empty means no limit.";
    case "max_vibrations":
      return "When even the best shaper leaves more vibration than this, prefer sharper corners. Empty uses the default.";
  }
}

const NOTHING_RECORDED: FitBaseline = {
  params: defaultFitParams,
  recorded: { scv: false, damping: false },
};

interface FieldProps {
  name: keyof FitDraft;
  value: string;
  hint: string;
  invalid: boolean;
  onChange: (value: string) => void;
}

/** A number field whose whole label, with an info icon, shows what it means when hovered. */
function FitField({ name, value, hint, invalid, onChange }: FieldProps) {
  const { label, unit, step } = FIELDS[name];
  const id = `fit-${name}`;
  return (
    <div className="space-y-1">
      <Tooltip>
        <TooltipTrigger render={<Label htmlFor={id} className="w-fit cursor-help" />}>
          {label}
          <InfoIcon aria-hidden="true" className="size-3.5 text-muted-foreground" />
        </TooltipTrigger>
        <TooltipContent side="top" className="max-w-64">
          {hint}
        </TooltipContent>
      </Tooltip>
      <UnitInput
        id={id}
        unit={unit}
        step={step}
        value={value}
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
  // The two limits start folded away, unless one is in use.
  const [advancedOpen] = useState(() => draft.max_smoothing !== "" || draft.max_vibrations !== "");
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
      onSubmit={(event) => {
        event.preventDefault();
        if ("params" in result) onApply(result.params);
      }}
    >
      <Panel title="Fit settings">
        <div className="grid gap-4 sm:grid-cols-2">
          {field("scv")}
          {field("damping_ratio")}
        </div>
        <Collapsible defaultOpen={advancedOpen} className="space-y-4">
          <CollapsibleTrigger
            render={
              <Button type="button" variant="ghost" size="sm" className="-ml-2 group/advanced text-muted-foreground" />
            }
          >
            Advanced
            <ChevronDownIcon className="transition-transform group-data-panel-open/advanced:rotate-180" />
          </CollapsibleTrigger>
          <CollapsibleContent keepMounted className="grid gap-4 sm:grid-cols-2">
            {field("max_smoothing")}
            {field("max_vibrations")}
          </CollapsibleContent>
        </Collapsible>
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
