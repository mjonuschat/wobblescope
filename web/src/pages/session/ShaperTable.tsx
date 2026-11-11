import { CopyButton } from "./CopyButton";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { formatAccel, formatHz } from "@/lib/format";
import type { Recommendation } from "@/lib/generated/Recommendation";

interface Props {
  recommendation: Recommendation;
  snippets?: string[] | null;
}

export function ShaperTable({ recommendation, snippets = null }: Props) {
  const { best, all } = recommendation;
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Shaper</TableHead>
          <TableHead>Frequency</TableHead>
          <TableHead>Vibrations</TableHead>
          <TableHead>Smoothing</TableHead>
          <TableHead>Max accel</TableHead>
          {snippets && (
            <TableHead className="w-0">
              <span className="sr-only">Copy the config</span>
            </TableHead>
          )}
        </TableRow>
      </TableHeader>
      <TableBody>
        {all.map((c, i) => {
          const isBest = c.name === best.name && c.freq === best.freq;
          return (
            <TableRow
              key={`${c.name}-${c.freq}`}
              data-best={isBest}
              className={isBest ? "bg-muted font-medium" : undefined}
            >
              <TableCell>{c.name}</TableCell>
              <TableCell>{formatHz(c.freq)} Hz</TableCell>
              <TableCell>{(c.vibrs * 100).toFixed(1)}%</TableCell>
              <TableCell>{c.smoothing.toFixed(3)} mm</TableCell>
              <TableCell>{formatAccel(c.max_accel)}</TableCell>
              {snippets && (
                <TableCell className="text-right">{snippets[i] && <CopyButton text={snippets[i]} />}</TableCell>
              )}
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}
