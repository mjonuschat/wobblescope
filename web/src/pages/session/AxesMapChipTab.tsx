import { Field, Fields } from "@/components/Fields";
import { Panel } from "@/components/Panel";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { AxesMapChip } from "@/lib/generated/AxesMapChip";
import type { AxesMapResult } from "@/lib/generated/AxesMapResult";
import type { AxisState } from "@/lib/generated/AxisState";
import type { ChipMeta } from "@/lib/generated/ChipMeta";
import type { ConfigCheck } from "@/lib/generated/ConfigCheck";

import { AxesMapPlot, chipAxisText, noiseText } from "./AxesMapPlot";
import { CopyButton } from "./CopyButton";
import { axesMapSnippet } from "./axesMapSnippet";

const STATE_WORDS: Record<AxisState, string> = {
  measured: "Measured",
  virtual: "Virtual",
  not_determined: "not determined",
};

const CONFIG_WORDS: Record<ConfigCheck, string> = {
  already_correct: "already correct",
  does_not_match: "does not match",
  none_configured: "none configured",
};

const degrees = (value: number) => `${value.toFixed(1)}°`;

function AxisTable({ result }: { result: AxesMapResult }) {
  return (
    <Table>
      <caption className="sr-only">Chip axis per machine axis</caption>
      <TableHeader>
        <TableRow>
          <TableHead>Machine axis</TableHead>
          <TableHead>Chip axis</TableHead>
          <TableHead>State</TableHead>
          <TableHead>Confidence</TableHead>
          <TableHead>Tilt</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {result.axes.map((match, i) => {
          const measured = match.state === "measured";
          return (
            <TableRow key={"XYZ"[i]}>
              <TableCell className="font-mono">{"XYZ"[i]}</TableCell>
              <TableCell className="font-mono">{match.chip_axis === null ? "–" : chipAxisText(match)}</TableCell>
              <TableCell>{STATE_WORDS[match.state]}</TableCell>
              <TableCell className="font-mono">{measured ? `${Math.round(match.confidence * 100)}%` : "–"}</TableCell>
              <TableCell className="font-mono">{measured ? degrees(match.tilt_deg) : "–"}</TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}

/** What the state of a result means for the snippet, in a sentence. */
function stateText(result: AxesMapResult): string | null {
  switch (result.state) {
    case "partial":
      return "Only some axes were determined. No snippet is offered.";
    case "no_motion":
      return "No motion seen by this chip. No snippet is offered.";
    case "ambiguous":
      return "Unable to determine correctly. No snippet is offered.";
    case "complete":
      return null;
  }
}

function Analysed({ outcome, chip }: { outcome: Extract<AxesMapChip, { state: "Analysed" }>; chip: ChipMeta | null }) {
  const { result } = outcome;
  // Only a complete map gets a snippet, so a partial or noise-built map never reaches the config.
  const snippet =
    result.state === "complete" && result.map !== null
      ? axesMapSnippet({ name: outcome.chip, chip_type: chip?.chip_type ?? null }, result.map)
      : null;
  const state = stateText(result);
  return (
    <>
      {snippet !== null && (
        <div className="flex items-start justify-between gap-4 rounded-lg border p-3">
          <pre className="font-mono text-sm">{snippet}</pre>
          <CopyButton text={snippet} />
        </div>
      )}
      {state && (
        <p role="status" className="text-sm">
          {state}
        </p>
      )}
      <Fields>
        <Field label="Configured">
          {result.configured_map ?? "none configured"}
          {snippet !== null && result.configured_map !== null && (
            <span className="text-muted-foreground"> ({CONFIG_WORDS[result.config_check]})</span>
          )}
        </Field>
        <Field label="Euler angles">
          {result.euler_deg ? result.euler_deg.map((a, i) => `${"XYZ"[i]} ${degrees(a)}`).join(", ") : "–"}
        </Field>
        <Field label="Gravity">{result.gravity.toFixed(3)} m/s²</Field>
        <Field label="Noise">{noiseText(result).replace(/^Noise: /, "")}</Field>
      </Fields>
      {result.quality.messages.length > 0 && (
        <ul aria-label="Quality messages" className="list-disc pl-5 text-sm">
          {result.quality.messages.map((m) => (
            <li key={m}>{m}</li>
          ))}
        </ul>
      )}
      <AxisTable result={result} />
      <AxesMapPlot result={result} />
    </>
  );
}

/** One chip's axes-map outcome: the snippet when there is one, and what the analysis found. */
export function AxesMapChipTab({ outcome, chip }: { outcome: AxesMapChip; chip: ChipMeta | null }) {
  // Sessions from before mountings were recorded are analysed as toolhead chips.
  const mounting = chip?.mounting ?? "toolhead";
  return (
    <Panel title="Axes Map" label={`Chip ${outcome.chip}`}>
      <Fields>
        <Field label="Mounted on">{mounting}</Field>
        {chip?.chip_type && <Field label="Type">{chip.chip_type}</Field>}
        {outcome.state === "Analysed" && (
          <Field label="Capture health" className="sm:col-span-2">
            {outcome.health}
            {outcome.health_notes.length > 0 && (
              <ul className="list-disc pl-5 font-sans text-muted-foreground">
                {outcome.health_notes.map((n) => (
                  <li key={n}>{n}</li>
                ))}
              </ul>
            )}
          </Field>
        )}
      </Fields>
      {outcome.state === "Analysed" && <Analysed outcome={outcome} chip={chip} />}
      {outcome.state === "Incomplete" && (
        <p role="status" className="text-sm">
          Incomplete capture: {outcome.missing}. This chip was not analysed and no snippet is offered.
        </p>
      )}
      {outcome.state === "Failed" && (
        <p role="alert" className="text-sm text-destructive">
          The analysis failed: {outcome.message}
        </p>
      )}
    </Panel>
  );
}
