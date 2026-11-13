import { pairLetter } from "@/components/charts/options";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { PeakOffset } from "@/lib/generated/PeakOffset";

/** One row per paired peak, named by the Greek letter the overlay marks it with. */
export function BeltOffsetTable({ offsets }: { offsets: PeakOffset[] }) {
  if (offsets.length === 0) {
    return <p className="text-sm">No peaks were paired across the belts.</p>;
  }
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Peaks</TableHead>
          <TableHead>Frequency delta</TableHead>
          <TableHead>Amplitude delta</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {offsets.map((offset, order) => (
          <TableRow key={pairLetter(order)}>
            <TableCell>{pairLetter(order)}</TableCell>
            <TableCell>{offset.freq_delta.toFixed(1)} Hz</TableCell>
            <TableCell>{offset.amplitude_delta_percent.toFixed(1)} %</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
