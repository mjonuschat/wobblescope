import type { ReactNode } from "react";

import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { Availability } from "@/lib/generated/Availability";
import type { Chip } from "@/lib/generated/Chip";

import type { Axis3 } from "./axes";

const AXIS_CHOICES: { axis: Axis3; label: string }[] = [
  { axis: "x", label: "X" },
  { axis: "y", label: "Y" },
  { axis: "z", label: "Z" },
  { axis: "sum", label: "Total" },
];

interface Props {
  chips: Chip[];
  selected: string[];
  maxChips: number;
  axes: Record<string, Axis3[]>;
  onToggle: (section: string) => void;
  onAxes: (section: string, axes: Axis3[]) => void;
  badge: (section: string) => ReactNode;
}

function check(availability: Availability): string {
  switch (availability.state) {
    case "unchecked":
      return "unchecked";
    case "available":
      return `available · ${Math.round(availability.observed_rate_hz)} Hz`;
    case "unavailable":
      return `unavailable: ${availability.reason}`;
  }
}

/** The chips to stream, at most `maxChips` at once, each with its own axis toggles. */
export function ChipPicker({ chips, selected, maxChips, axes, onToggle, onAxes, badge }: Props) {
  const full = selected.length >= maxChips;
  return (
    <ul className="space-y-3">
      {chips.map((chip) => {
        const on = selected.includes(chip.section);
        const blocked = !on && full;
        const chosen = axes[chip.section] ?? ["sum"];
        return (
          <li key={chip.section} className="space-y-2 rounded-md border p-3">
            <div className="flex flex-wrap items-center gap-3">
              <div className="flex items-center gap-2">
                <Checkbox
                  id={`stream-${chip.section}`}
                  aria-label={`Stream ${chip.section}`}
                  checked={on}
                  disabled={blocked}
                  onCheckedChange={() => onToggle(chip.section)}
                />
                <Label htmlFor={`stream-${chip.section}`}>{chip.section}</Label>
              </div>
              <Badge variant="outline">{chip.chip_type}</Badge>
              <span className="text-sm text-muted-foreground">
                {chip.nominal_rate_hz === null ? "rate unknown" : `${chip.nominal_rate_hz} Hz`}
              </span>
              <span className="text-sm text-muted-foreground">{check(chip.availability)}</span>
              {blocked && <span className="text-sm text-amber-400">at most {maxChips} chips at once</span>}
            </div>
            {on && (
              <div className="space-y-2">
                <ToggleGroup
                  multiple
                  variant="outline"
                  size="sm"
                  aria-label={`Axes of ${chip.section}`}
                  value={chosen}
                  onValueChange={(next) => {
                    if (next.length > 0) onAxes(chip.section, next as Axis3[]);
                  }}
                >
                  {AXIS_CHOICES.map(({ axis, label }) => (
                    <ToggleGroupItem key={axis} value={axis} aria-label={`${label} of ${chip.section}`}>
                      {label}
                    </ToggleGroupItem>
                  ))}
                </ToggleGroup>
                {badge(chip.section)}
              </div>
            )}
          </li>
        );
      })}
    </ul>
  );
}
