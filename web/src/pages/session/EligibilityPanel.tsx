import { CircleCheckIcon, OctagonXIcon, TriangleAlertIcon } from "lucide-react";

import { Panel } from "@/ui-kit/Panel";
import { pointLabel } from "@/components/charts/options";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import type { Analysis } from "@/lib/generated/Analysis";
import type { DatasetDecision } from "@/lib/generated/DatasetDecision";
import type { ProbePoint } from "@/lib/generated/ProbePoint";
import { Alert } from "@/ui-kit/Alert";

/** The chips with a dataset that health or coverage excludes, or that an override includes. */
function overridable(analysis: Analysis) {
  const chips = new Map<string, { overridden: boolean }>();
  for (const d of analysis.eligibility.decisions) {
    if (d.decision === "Included") continue;
    const entry = chips.get(d.chip) ?? { overridden: false };
    entry.overridden ||= d.decision === "Overridden";
    chips.set(d.chip, entry);
  }
  return chips;
}

export function explainReason(reason: string): string {
  if (reason === "degraded capture health") return "The capture looks unreliable. The health notes say why.";
  const missing = /^incomplete coverage: (.*)$/.exec(reason)?.[1];
  if (missing) return `The capture is incomplete: ${missing}`;
  return reason;
}

type Tone = "good" | "warn" | "bad";

export function verdict(d: DatasetDecision): { tone: Tone; headline: string; detail: string } {
  const detail = d.reasons.map(explainReason).join(" ");
  if (d.decision === "Excluded") return { tone: "bad", headline: "Left out", detail };
  if (d.decision === "Overridden") return { tone: "warn", headline: "Included at your request", detail };
  return { tone: "good", headline: "Good data", detail: "" };
}

const TONES = {
  good: { Icon: CircleCheckIcon, className: "text-success" },
  warn: { Icon: TriangleAlertIcon, className: "text-warning" },
  bad: { Icon: OctagonXIcon, className: "text-destructive" },
} as const;

export function EligibilityPanel({
  analysis,
  probePoints,
  overrides,
  onOverridesChange,
  readOnly = false,
}: {
  analysis: Analysis;
  probePoints: ProbePoint[];
  overrides: string[];
  onOverridesChange?: (overrides: string[]) => void;
  readOnly?: boolean;
}) {
  const { decisions } = analysis.eligibility;
  if (decisions.length === 0 && analysis.failures.length === 0) return null;
  const manyPoints = new Set(decisions.map((d) => d.point)).size > 1;
  const overridableChips = overridable(analysis);
  // The override is per chip, so its checkbox sits under the chip's first decision that needs it.
  const checkboxAt = new Map<string, number>();
  decisions.forEach((d, index) => {
    if (d.decision !== "Included" && !checkboxAt.has(d.chip)) checkboxAt.set(d.chip, index);
  });
  return (
    <Panel title="Data quality">
      <ul className="flex flex-wrap items-start gap-x-3 gap-y-3">
        {decisions.map((d, index) => {
          const { tone, headline, detail } = verdict(d);
          const { Icon, className } = TONES[tone];
          return (
            <li key={`${d.chip}-${d.point}`} className="flex items-start gap-3">
              {index > 0 && (
                <span aria-hidden="true" className="text-muted-foreground">
                  ·
                </span>
              )}
              <Icon aria-hidden="true" className={`mt-0.5 size-5 shrink-0 ${className}`} />
              <div className="space-y-0.5">
                <p>
                  <span className="font-mono font-medium">{d.chip}</span>
                  {manyPoints && (
                    <span className="text-sm text-muted-foreground"> at {pointLabel(d.point, probePoints)}</span>
                  )}
                  : {headline}
                </p>
                {detail && <p className="text-sm text-muted-foreground">{detail}</p>}
                {d.dropped !== null && (
                  <Alert tone="warning" variant="inline">
                    Could not be analysed: {d.dropped}
                  </Alert>
                )}
                {checkboxAt.get(d.chip) === index && (
                  <div className="flex items-center gap-2 pt-1 text-sm">
                    <Checkbox
                      id={`include-${d.chip}`}
                      checked={overridableChips.get(d.chip)?.overridden ?? false}
                      disabled={readOnly}
                      onCheckedChange={(checked) =>
                        onOverridesChange?.(checked ? [...overrides, d.chip] : overrides.filter((c) => c !== d.chip))
                      }
                    />
                    <Label htmlFor={`include-${d.chip}`} className="text-foreground">
                      Include {d.chip} anyway
                    </Label>
                  </div>
                )}
              </div>
            </li>
          );
        })}
      </ul>
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
