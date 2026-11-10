import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { Availability } from "@/lib/generated/Availability";
import type { ChipsSnapshot } from "@/lib/generated/ChipsSnapshot";

interface Props {
  chips: ChipsSnapshot;
  /** The chips that will record. */
  selected: string[];
  onToggle: (section: string) => void;
  onCheck: () => void;
  checking: boolean;
  /** Why a check cannot run now, or `null`. */
  checkBlocked: string | null;
}

function AvailabilityCell({ availability }: { availability: Availability }) {
  switch (availability.state) {
    case "unchecked":
      return <Badge variant="outline">unchecked</Badge>;
    case "available":
      return <Badge variant="secondary">available · {Math.round(availability.observed_rate_hz)} Hz</Badge>;
    case "unavailable":
      return (
        <div className="space-y-1">
          <Badge variant="destructive">unavailable</Badge>
          <p className="text-xs text-muted-foreground">{availability.reason}</p>
        </div>
      );
  }
}

function names(list: string[]): string {
  return list.length > 0 ? list.join(", ") : "none";
}

/** The configured chips with their availability, the defaults and the sections left out. */
export function ChipsPanel({ chips, selected, onToggle, onCheck, checking, checkBlocked }: Props) {
  return (
    <section aria-labelledby="chips-heading" className="space-y-3">
      <div className="flex items-center justify-between gap-4">
        <h2 id="chips-heading" className="text-lg font-semibold">
          Chips
        </h2>
        <Button
          type="button"
          variant="outline"
          onClick={onCheck}
          disabled={checking || checkBlocked !== null}
          title={checkBlocked ?? undefined}
        >
          {checking ? "Checking chips…" : "Check chips"}
        </Button>
      </div>
      {checkBlocked && <p className="text-sm text-muted-foreground">{checkBlocked}</p>}
      {!chips.known && (
        <p className="text-sm text-muted-foreground">The daemon has not read the chip list from Klipper yet.</p>
      )}
      {chips.known && !chips.current && (
        <p className="text-sm text-muted-foreground">
          This is the last list the daemon read. It is read again when Klipper is ready.
        </p>
      )}
      {chips.chips.length > 0 && (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Use</TableHead>
              <TableHead>Chip</TableHead>
              <TableHead>Type</TableHead>
              <TableHead>Rate</TableHead>
              <TableHead>Availability</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {chips.chips.map((chip) => (
              <TableRow key={chip.section}>
                <TableCell>
                  <input
                    type="checkbox"
                    aria-label={`Use ${chip.section}`}
                    checked={selected.includes(chip.section)}
                    disabled={chip.availability.state === "unavailable"}
                    onChange={() => onToggle(chip.section)}
                  />
                </TableCell>
                <TableCell>{chip.section}</TableCell>
                <TableCell>{chip.chip_type}</TableCell>
                <TableCell>{chip.nominal_rate_hz === null ? "—" : `${chip.nominal_rate_hz} Hz`}</TableCell>
                <TableCell>
                  <AvailabilityCell availability={chip.availability} />
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
      {chips.known && chips.chips.length === 0 && (
        <p className="text-sm">Klipper's config lists no supported accelerometer.</p>
      )}
      {chips.known && (
        <p className="text-sm text-muted-foreground">
          Klipper's defaults: X uses {names(chips.defaults.x)}; Y uses {names(chips.defaults.y)}.
        </p>
      )}
      {chips.skipped.length > 0 && (
        <div className="space-y-1 text-sm">
          <h3 className="font-medium">Left out</h3>
          <ul className="list-disc pl-5 text-muted-foreground">
            {chips.skipped.map((s) => (
              <li key={s.section}>
                {s.section}: {s.reason}
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
