import type { ReactNode } from "react";

import { Switch } from "@/components/ui/switch";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
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

/** The highest frequency a chip can show, which is half its sample rate. */
function maxFrequency(chip: Chip): string {
  return chip.nominal_rate_hz === null ? "unknown" : `${Math.floor(chip.nominal_rate_hz / 2)} Hz`;
}

/** The chips to stream, at most `maxChips` at once, each with its own axis toggles. */
export function ChipPicker({ chips, selected, maxChips, axes, onToggle, onAxes, badge }: Props) {
  const full = selected.length >= maxChips;
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead className="w-14">Show</TableHead>
          <TableHead>Chip</TableHead>
          <TableHead>Type</TableHead>
          <TableHead>Rate</TableHead>
          <TableHead>Max Frequency</TableHead>
          <TableHead>Axes</TableHead>
          <TableHead>Status</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {chips.map((chip) => {
          const on = selected.includes(chip.section);
          const blocked = !on && full;
          const chosen = axes[chip.section] ?? ["sum"];
          return (
            <TableRow key={chip.section}>
              <TableCell>
                <Switch
                  aria-label={`Stream ${chip.section}`}
                  checked={on}
                  disabled={blocked}
                  onCheckedChange={() => onToggle(chip.section)}
                />
              </TableCell>
              <TableCell className="font-mono">{chip.section}</TableCell>
              <TableCell>{chip.chip_type}</TableCell>
              <TableCell className="font-mono">
                {chip.nominal_rate_hz === null ? "unknown" : `${chip.nominal_rate_hz} Hz`}
              </TableCell>
              <TableCell className="font-mono">{maxFrequency(chip)}</TableCell>
              <TableCell>
                {on && (
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
                )}
              </TableCell>
              <TableCell>
                {on && badge(chip.section)}
                {!on && chip.availability.state === "unavailable" && (
                  <span className="text-sm text-destructive">unavailable: {chip.availability.reason}</span>
                )}
                {blocked && <span className="text-sm text-amber-400">at most {maxChips} chips at once</span>}
              </TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}
