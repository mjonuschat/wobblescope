import { Panel } from "@/components/Panel";
import { pointLabel } from "@/components/charts/options";
import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import type { Analysis } from "@/lib/generated/Analysis";
import type { ProbePoint } from "@/lib/generated/ProbePoint";

/** The chips with a dataset that health or coverage excludes, or that an override includes. */
function overridable(analysis: Analysis) {
  const chips = new Map<string, { overridden: boolean; reasons: string[] }>();
  for (const d of analysis.eligibility.decisions) {
    if (d.decision === "Included") continue;
    const entry = chips.get(d.chip) ?? { overridden: false, reasons: [] };
    entry.overridden ||= d.decision === "Overridden";
    for (const reason of d.reasons) if (!entry.reasons.includes(reason)) entry.reasons.push(reason);
    chips.set(d.chip, entry);
  }
  return chips;
}

/** The reasons the analysis gives, in words for someone who has not read its source. */
export function explainReason(reason: string): string {
  if (reason === "degraded capture health") return "The chip dropped or garbled samples during the capture.";
  if (reason === "unknown capture health") {
    return "Health unknown. This recording has no record of whether samples were dropped, so it cannot be checked.";
  }
  const missing = /^incomplete coverage: (.*)$/.exec(reason)?.[1];
  if (missing) return `The capture is incomplete: ${missing}`;
  return reason;
}

/** Where a recording was made, as "test point (x, y, z)". */
function whereText(point: number, probePoints: ProbePoint[]): string {
  const label = pointLabel(point, probePoints);
  return label.startsWith("(") ? `test point ${label}` : label;
}

const STATUS = {
  Included: { text: "Used", className: "border-emerald-500/50 text-emerald-400" },
  Overridden: { text: "Included by you", className: "border-amber-500/50 text-amber-400" },
  Excluded: { text: "Left out", className: "border-red-500/50 text-red-400" },
} as const;

export function EligibilityPanel({
  analysis,
  probePoints,
  overrides,
  onOverridesChange,
}: {
  analysis: Analysis;
  probePoints: ProbePoint[];
  overrides: string[];
  onOverridesChange: (overrides: string[]) => void;
}) {
  const { decisions } = analysis.eligibility;
  if (decisions.length === 0 && analysis.failures.length === 0) return null;
  return (
    <Panel title="Data Quality">
      <p className="text-sm text-muted-foreground">
        How good the recordings are, and which of them went into the recommendation. A recording with a problem is left
        out unless you include it.
      </p>
      <ul className="space-y-3">
        {decisions.map((d) => {
          const { text, className } = STATUS[d.decision];
          return (
            <li key={`${d.chip}-${d.point}`} className="space-y-1">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-mono font-medium">{d.chip}</span>
                <span className="text-sm text-muted-foreground">{whereText(d.point, probePoints)}</span>
                <Badge variant="outline" className={className}>
                  {text}
                </Badge>
              </div>
              {d.reasons.map((reason) => (
                <p
                  key={reason}
                  className={`text-sm ${d.decision === "Excluded" ? "text-red-400" : "text-muted-foreground"}`}
                >
                  {explainReason(reason)}
                </p>
              ))}
              {d.dropped !== null && <p className="text-sm text-amber-400">Could not be analysed: {d.dropped}</p>}
            </li>
          );
        })}
      </ul>
      {[...overridable(analysis)].map(([chip, { overridden, reasons }]) => (
        <div key={chip} className="flex items-start gap-2 text-sm">
          <Checkbox
            id={`include-${chip}`}
            className="mt-0.5"
            checked={overridden}
            onCheckedChange={(checked) =>
              onOverridesChange(checked ? [...overrides, chip] : overrides.filter((c) => c !== chip))
            }
          />
          <Label htmlFor={`include-${chip}`} className="items-start font-normal">
            <span>
              Include {chip} anyway
              <span className="text-muted-foreground"> ({reasons.join("; ")})</span>
            </span>
          </Label>
        </div>
      ))}
      {analysis.failures.length > 0 && (
        <ul className="space-y-1 text-sm text-destructive">
          {analysis.failures.map((f) => (
            <li key={`${f.chip}-${f.point}`}>
              {f.chip} at {pointLabel(f.point, probePoints)} could not be processed: {f.error}
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}
