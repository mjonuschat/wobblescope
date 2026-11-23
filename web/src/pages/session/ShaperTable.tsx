import { TriangleAlertIcon } from "lucide-react";

import { CopyButton } from "./CopyButton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { formatAccel, formatHz } from "@/lib/format";
import type { Recommendation } from "@/lib/generated/Recommendation";

interface Props {
  recommendation: Recommendation;
  snippets?: string[] | null;
}

export function ShaperTable({ recommendation, snippets = null }: Props) {
  const { best, all, for_performance: performance, unfitted, settings } = recommendation;
  const hasActions = snippets !== null || unfitted.length > 0;
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Shaper</TableHead>
          <TableHead>Frequency</TableHead>
          <TableHead>Vibrations</TableHead>
          <TableHead>Smoothing</TableHead>
          <TableHead>Max accel</TableHead>
          {hasActions && (
            <TableHead className="w-0">
              <span className="sr-only">Copy the config</span>
            </TableHead>
          )}
        </TableRow>
      </TableHeader>
      <TableBody>
        {all.map((c, i) => {
          const isBest = c.name === best.name && c.freq === best.freq;
          const isPerformance = performance !== null && c.name === performance.name && c.freq === performance.freq;
          return (
            <TableRow
              key={`${c.name}-${c.freq}`}
              data-best={isBest}
              data-performance={isPerformance}
              className={isBest ? "bg-muted font-medium" : undefined}
            >
              <TableCell>
                {c.name}
                {isPerformance && (
                  <span className="ml-2 text-xs font-normal text-muted-foreground">for performance</span>
                )}
              </TableCell>
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
        {unfitted.map((u) => (
          <TableRow key={u.name} data-unfitted="true" className="text-muted-foreground">
            <TableCell>{u.name}</TableCell>
            <TableCell>-</TableCell>
            <TableCell>-</TableCell>
            <TableCell>-</TableCell>
            <TableCell>-</TableCell>
            <TableCell className="text-right">
              <Tooltip>
                <TooltipTrigger
                  render={
                    <span
                      role="img"
                      tabIndex={0}
                      aria-label={`${u.name} not fitted: damping ratio ${settings.damping_ratio.toFixed(3)} is above this shaper's limit of ${u.max_damping}`}
                      className="inline-flex size-8 items-center justify-center text-destructive"
                    />
                  }
                >
                  <TriangleAlertIcon className="size-4" />
                </TooltipTrigger>
                <TooltipContent side="top" className="max-w-64">
                  Not fitted: damping ratio {settings.damping_ratio.toFixed(3)} is above this shaper&apos;s limit of{" "}
                  {u.max_damping}.
                </TooltipContent>
              </Tooltip>
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
