import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { VibrationsResult } from "@/lib/generated/VibrationsResult";
import type { Zone } from "@/lib/generated/Zone";

import { subheadingClass } from "@/ui-kit/CardLabel";

const round1 = (value: number) => Number(value.toFixed(1));

/** The index of a zone's end can point one past the grid. Keep it on it. */
const at = (values: number[], index: number) => values[Math.max(0, Math.min(index, values.length - 1))];

const heading = subheadingClass;

function Badge({ n }: { n: number }) {
  return (
    <span className="inline-flex size-5 items-center justify-center rounded-full bg-success text-[11px] font-medium text-success-foreground">
      {n}
    </span>
  );
}

function ZoneTable({
  title,
  unit,
  empty,
  zones,
  range,
}: {
  title: string;
  unit: string;
  empty: string;
  zones: Zone[];
  range: (zone: Zone) => string;
}) {
  return (
    <div className="space-y-2">
      <h3 className={heading}>{title}</h3>
      {zones.length === 0 ? (
        <p className="text-sm">{empty}</p>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-10">#</TableHead>
              <TableHead>{`Range (${unit})`}</TableHead>
              <TableHead className="text-right">Avg. vibration (% of max)</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {zones.map((zone, index) => (
              <TableRow key={`${zone.start}-${zone.end}`}>
                <TableCell>
                  <Badge n={index + 1} />
                </TableCell>
                <TableCell className="tabular-nums">{range(zone)}</TableCell>
                <TableCell className="text-right tabular-nums">{zone.energy.toFixed(1)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </div>
  );
}

export function GoodSpeedRanges({ result }: { result: VibrationsResult }) {
  return (
    <ZoneTable
      title="Recommended speed ranges"
      unit="mm/s"
      empty="No recommended speed ranges were found."
      zones={result.good_speeds}
      range={(zone) => `${round1(at(result.speeds, zone.start))} to ${round1(at(result.speeds, zone.end))}`}
    />
  );
}

export function GoodAngleRanges({ result }: { result: VibrationsResult }) {
  return (
    <ZoneTable
      title="Good angle ranges"
      unit="degrees"
      empty="No good angle ranges were found."
      zones={result.good_angles}
      range={(zone) => `${round1(at(result.angles, zone.start))} to ${round1(at(result.angles, zone.end))}`}
    />
  );
}

export function VibrationsSummary({ result }: { result: VibrationsResult }) {
  return <GoodSpeedRanges result={result} />;
}
