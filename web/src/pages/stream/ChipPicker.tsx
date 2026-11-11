import type { ReactNode } from "react";

import { Switch } from "@/components/ui/switch";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { Chip } from "@/lib/generated/Chip";

interface Props {
  chips: Chip[];
  selected: string[];
  maxChips: number;
  onToggle: (section: string) => void;
  badge: (section: string) => ReactNode;
}

/** The highest frequency a chip can show, which is half its sample rate. */
function maxFrequency(chip: Chip): string {
  return chip.nominal_rate_hz === null ? "unknown" : `${Math.floor(chip.nominal_rate_hz / 2)} Hz`;
}

/** The chips to stream, at most `maxChips` at once. */
export function ChipPicker({ chips, selected, maxChips, onToggle, badge }: Props) {
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
          <TableHead>Status</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {chips.map((chip) => {
          const on = selected.includes(chip.section);
          const blocked = !on && full;
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
