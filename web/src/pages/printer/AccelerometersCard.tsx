import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { Availability } from "@/lib/generated/Availability";
import type { Chip } from "@/lib/generated/Chip";
import type { ChipsSnapshot } from "@/lib/generated/ChipsSnapshot";

interface Props {
  chips: ChipsSnapshot;
  onCheck: () => void;
  checking: boolean;
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

function defaultAxes(chips: ChipsSnapshot, chip: Chip): string {
  const axes = (["x", "y"] as const)
    .filter((axis) => chips.defaults[axis].includes(chip.section))
    .map((axis) => axis.toUpperCase());
  return axes.length > 0 ? axes.join(" and ") : "none";
}

export function AccelerometersCard({ chips, onCheck, checking, checkBlocked }: Props) {
  return (
    <Card>
      <CardHeader className="flex items-center justify-between gap-4">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Accelerometers
        </CardTitle>
        <Button
          type="button"
          variant="outline"
          onClick={onCheck}
          disabled={checking || checkBlocked !== null}
          title={checkBlocked ?? undefined}
        >
          {checking ? "Checking chips…" : "Check chips"}
        </Button>
      </CardHeader>
      <CardContent className="space-y-3">
        {checkBlocked && <p className="text-sm text-muted-foreground">{checkBlocked}</p>}
        {!chips.known && (
          <p className="text-sm text-muted-foreground">The daemon has not read the chip list from Klipper yet.</p>
        )}
        {chips.known && !chips.current && (
          <p className="text-sm text-muted-foreground">
            This is the last list the daemon read. It is read again when Klipper is ready.
          </p>
        )}
        {chips.known && chips.chips.length === 0 && (
          <p className="text-sm">Klipper's config lists no supported accelerometer.</p>
        )}
        {chips.chips.length > 0 && (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Chip</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>MCU</TableHead>
                <TableHead>Rate</TableHead>
                <TableHead>Default for</TableHead>
                <TableHead>Availability</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {chips.chips.map((chip) => (
                <TableRow key={chip.section}>
                  <TableCell className="font-mono">{chip.section}</TableCell>
                  <TableCell>{chip.chip_type}</TableCell>
                  <TableCell>{chip.mcu ?? "unknown"}</TableCell>
                  <TableCell className="font-mono">
                    {chip.nominal_rate_hz === null ? "unknown" : `${chip.nominal_rate_hz} Hz`}
                  </TableCell>
                  <TableCell>{defaultAxes(chips, chip)}</TableCell>
                  <TableCell>
                    <AvailabilityCell availability={chip.availability} />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
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
      </CardContent>
    </Card>
  );
}
