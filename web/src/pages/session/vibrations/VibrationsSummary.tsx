import { useState } from "react";

import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
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

type Order = "position" | "quality";

/** Lists the zones in the order of their position or of how quiet they are, the quietest first. A
 * zone keeps the number it has in the plot, which counts them by position. */
function ZoneTable({
  title,
  unit,
  empty,
  positionLabel,
  zones,
  range,
}: {
  title: string;
  unit: string;
  empty: string;
  positionLabel: string;
  zones: Zone[];
  range: (zone: Zone) => string;
}) {
  const [order, setOrder] = useState<Order>("position");
  const rows = zones.map((zone, index) => ({ zone, number: index + 1 }));
  if (order === "quality") rows.sort((a, b) => a.zone.energy - b.zone.energy || a.number - b.number);
  return (
    <section className="space-y-2">
      <div className="flex items-center justify-between gap-3">
        <h3 className={heading}>{title}</h3>
        {zones.length > 1 && (
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            Sort by
            <ToggleGroup
              variant="outline"
              size="sm"
              aria-label={`Sort ${title.toLowerCase()}`}
              value={[order]}
              onValueChange={(next) => {
                if (next.length > 0) setOrder(next[0] as Order);
              }}
            >
              <ToggleGroupItem value="position">{positionLabel}</ToggleGroupItem>
              <ToggleGroupItem value="quality">Quality</ToggleGroupItem>
            </ToggleGroup>
          </div>
        )}
      </div>
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
            {rows.map(({ zone, number }) => (
              <TableRow key={`${zone.start}-${zone.end}`}>
                <TableCell>
                  <Badge n={number} />
                </TableCell>
                <TableCell className="tabular-nums">{range(zone)}</TableCell>
                <TableCell className="text-right tabular-nums">{zone.energy.toFixed(1)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </section>
  );
}

export function GoodSpeedRanges({ result }: { result: VibrationsResult }) {
  return (
    <ZoneTable
      title="Recommended speed ranges"
      unit="mm/s"
      empty="No recommended speed ranges were found."
      positionLabel="Speed"
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
      positionLabel="Angle"
      zones={result.good_angles}
      range={(zone) => `${round1(at(result.angles, zone.start))} to ${round1(at(result.angles, zone.end))}`}
    />
  );
}

export function VibrationsSummary({ result }: { result: VibrationsResult }) {
  return <GoodSpeedRanges result={result} />;
}
