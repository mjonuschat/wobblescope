import { Alert } from "@/ui-kit/Alert";
import { Field, Fields } from "@/ui-kit/Fields";
import { Panel } from "@/ui-kit/Panel";
import { Dot, type Tone } from "@/components/StatusDots";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { AxesMapChip } from "@/lib/generated/AxesMapChip";
import type { AxesMapResult } from "@/lib/generated/AxesMapResult";
import type { AxisState } from "@/lib/generated/AxisState";
import type { ChipMeta } from "@/lib/generated/ChipMeta";
import type { ConfigCheck } from "@/lib/generated/ConfigCheck";
import type { QualityStatus } from "@/lib/generated/QualityStatus";

import { mountingLabel } from "@/lib/mounting";

import { AxesMapPlot, chipAxisText } from "./AxesMapPlot";
import { CopyButton } from "./CopyButton";
import { formatMap, noiseText } from "./axesMapFormat";
import { axesMapSnippet } from "./axesMapSnippet";

const STATE_WORDS: Record<AxisState, string> = {
  measured: "Measured",
  virtual: "Inferred",
  not_determined: "not determined",
};

const CONFIG_WORDS: Record<ConfigCheck, string> = {
  already_correct: "matches the detected map",
  does_not_match: "differs from the detected map",
};

const QUALITY_WORDS: Record<QualityStatus, { word: string; tone: Tone }> = {
  Ok: { word: "Good", tone: "ok" },
  Warning: { word: "Warning", tone: "warn" },
  Error: { word: "Error", tone: "error" },
};

/** The map Klipper uses for a chip with no `axes_map` set. */
const DEFAULT_MAP = "x, y, z";

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

function snippetOf(outcome: Extract<AxesMapChip, { state: "Analysed" }>, chip: ChipMeta | null): string | null {
  const { result } = outcome;
  // Only a complete map gets a snippet, so a partial or noise-built map never reaches the config.
  return result.state === "complete" && result.map !== null
    ? axesMapSnippet({ name: outcome.chip, chip_type: chip?.chip_type ?? null }, result.map)
    : null;
}

export function AxesMapSummary({
  outcome,
  chip,
}: {
  outcome: Extract<AxesMapChip, { state: "Analysed" }>;
  chip: ChipMeta | null;
}) {
  const { result } = outcome;
  const snippet = snippetOf(outcome, chip);
  const state = stateText(result);
  return (
    <div className="space-y-4">
      <Fields>
        <Field label="Detected map">{result.map === null ? "not determined" : formatMap(result.map)}</Field>
        <Field label="Configured map" className="sm:col-span-2">
          {result.configured_map === null ? DEFAULT_MAP : formatMap(result.configured_map)}
          {result.config_check !== null && (
            <span className="font-sans text-muted-foreground"> ({CONFIG_WORDS[result.config_check]})</span>
          )}
        </Field>
      </Fields>
      {snippet !== null && (
        <div className="space-y-1">
          <div className="flex items-start justify-between gap-4 rounded-lg border p-3">
            <pre className="font-mono text-sm">{snippet}</pre>
            <CopyButton text={snippet} />
          </div>
          {result.config_check === "already_correct" && (
            <p className="text-sm text-muted-foreground">No change needed.</p>
          )}
        </div>
      )}
      {(state !== null || result.quality.status !== "Ok") && (
        <Alert
          tone={result.quality.status === "Error" ? "danger" : "warning"}
          title={`Detection quality: ${QUALITY_WORDS[result.quality.status].word}`}
        >
          {state && <p>{state}</p>}
          {result.quality.status !== "Ok" && result.quality.messages.map((m) => <p key={m}>{m}</p>)}
        </Alert>
      )}
    </div>
  );
}

type AnalysedOutcome = Extract<AxesMapChip, { state: "Analysed" }>;

function AnalysedFields({ outcome }: { outcome: AnalysedOutcome }) {
  const { result } = outcome;
  const quality = QUALITY_WORDS[result.quality.status];
  return (
    <>
      <Field label="Capture health">{outcome.health}</Field>
      <Field label="Detection quality">
        <span className="inline-flex items-center gap-2">
          <Dot tone={quality.tone} />
          {quality.word}
        </span>
      </Field>
      <Field label="Noise">{noiseText(result)}</Field>
    </>
  );
}

export function AxesMapChipTab({ outcome, chip }: { outcome: AxesMapChip; chip: ChipMeta | null }) {
  // Sessions from before mountings were recorded are analysed as toolhead chips.
  const mounting = chip?.mounting ?? "toolhead";
  return (
    <Panel title="Axes map" label={`Chip ${outcome.chip}`}>
      <Fields className="lg:grid-cols-5">
        <Field label="Mounted on">{mountingLabel(mounting)}</Field>
        {chip?.chip_type && <Field label="Type">{chip.chip_type}</Field>}
        {outcome.state === "Analysed" && <AnalysedFields outcome={outcome} />}
      </Fields>
      {outcome.state === "Analysed" && (
        <>
          {outcome.health_notes.length > 0 && (
            <ul aria-label="Capture health notes" className="list-disc pl-5 text-sm text-muted-foreground">
              {outcome.health_notes.map((n) => (
                <li key={n}>{n}</li>
              ))}
            </ul>
          )}
          <AxisTable result={outcome.result} />
          <AxesMapPlot result={outcome.result} />
        </>
      )}
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
