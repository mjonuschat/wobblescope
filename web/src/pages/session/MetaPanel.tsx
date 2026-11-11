import { ChevronDownIcon } from "lucide-react";
import { useState } from "react";

import { Field, Fields } from "@/components/Fields";
import { Panel } from "@/components/Panel";
import { pointLabel } from "@/components/charts/options";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { formatTime } from "@/lib/format";
import type { Analysis } from "@/lib/generated/Analysis";
import type { Outcome } from "@/lib/generated/Outcome";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

import { effectiveSettings } from "./fit";

function outcomeText(outcome: Outcome): string {
  if (outcome === "Complete") return "Complete";
  if ("Incomplete" in outcome) return `Incomplete: ${outcome.Incomplete.reason}`;
  return `Failed: ${outcome.Failed.reason}`;
}

/** Where the session came from: this app's own test, or a CSV file. */
function sourceText(meta: SessionMeta): string {
  return meta.source === "Capture" ? "Wobblescope" : "CSV";
}

/** The name of the file an import came from, when it has one. A test has no file. */
function fileName(meta: SessionMeta): string | null {
  return meta.source === "Capture" ? null : meta.source.CsvImport.original_file_name;
}

const row = (label: string, value: React.ReactNode, className?: string) => (
  <Field key={label} label={label} className={className}>
    {value}
  </Field>
);

/** A position as `X: 150.5, Y: 154, Z: 50`. */
function positionText(position: [number, number, number] | null): string {
  if (!position) return "Not recorded";
  const [x, y, z] = position;
  return `X: ${x}, Y: ${y}, Z: ${z}`;
}

/**
 * The test points: the first on one line, and when there are more a button that opens them all,
 * one per line below.
 */
function ProbePoints({ points }: { points: SessionMeta["probe_points"] }) {
  const [open, setOpen] = useState(false);
  if (points.length === 0) return <>Not recorded</>;
  if (points.length === 1) return <>{positionText(points[0].position)}</>;
  return (
    <div>
      <div className="flex items-center gap-2">
        {open ? (
          <span className="text-muted-foreground">{points.length} points</span>
        ) : (
          positionText(points[0].position)
        )}
        <button
          type="button"
          aria-expanded={open}
          aria-label={open ? "Show only the first point" : `Show all ${points.length} points`}
          onClick={() => setOpen(!open)}
          className="rounded p-0.5 text-muted-foreground hover:text-foreground"
        >
          <ChevronDownIcon aria-hidden="true" className={`size-4 transition-transform ${open ? "rotate-180" : ""}`} />
        </button>
      </div>
      {open && (
        <ol className="mt-1 space-y-0.5">
          {points.map((p, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: points are recorded in order and have no id
            <li key={i}>
              <span className="text-muted-foreground">{i + 1}</span> {positionText(p.position)}
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

/** The recordings whose own segment showed a problem, which the chip's overall health hides. */
function degradedSegments(meta: SessionMeta) {
  return meta.segments.flatMap((segment, index) =>
    segment.files
      .filter((f) => f.health === "Degraded")
      .map((f) => ({
        key: `${index}-${f.chip}`,
        where: `Segment ${index + 1} (${segment.axis}, ${pointLabel(segment.point, meta.probe_points)})`,
        chip: meta.chips[f.chip]?.name ?? `chip ${f.chip}`,
        notes: f.health_notes,
      })),
  );
}

export function MetaPanel({ meta, analysis = null }: { meta: SessionMeta; analysis?: Analysis | null }) {
  const fit = analysis ? effectiveSettings(analysis) : null;
  const s = meta.settings;
  const degraded = degradedSegments(meta);
  return (
    <Panel title="Session" label="Session details">
      <Fields>
        {row("Source", sourceText(meta))}
        {fileName(meta) !== null && row("File Name", fileName(meta))}
        {row("Created", formatTime(meta.created_at))}
        {row("Mode", meta.mode ?? "Not recorded")}
        {row("Printer", meta.printer?.name ?? "Not recorded")}
        {meta.printer?.klipper_version && row("Klipper", meta.printer.klipper_version)}
        {meta.printer?.square_corner_velocity != null &&
          row("Square corner velocity", `${meta.printer.square_corner_velocity} mm/s`)}
        {meta.printer?.damping_ratio_x != null && row("Damping ratio X", String(meta.printer.damping_ratio_x))}
        {meta.printer?.damping_ratio_y != null && row("Damping ratio Y", String(meta.printer.damping_ratio_y))}
        {meta.printer &&
          meta.printer.mcu_firmware.length > 0 &&
          row(
            "MCU firmware",
            <ul>
              {meta.printer.mcu_firmware.map((f) => (
                <li key={f}>{f}</li>
              ))}
            </ul>,
          )}
        <Field label="Probe Points">
          <ProbePoints points={meta.probe_points} />
        </Field>
        {/* The last item, in the fourth column where there are four. */}
        {row("Outcome", outcomeText(meta.outcome), "lg:col-start-4")}
      </Fields>
      {s && (
        <Fields aria-label="Recorded sweep settings">
          {row("Frequency range", `${s.freq_start} to ${s.freq_end} Hz`)}
          {row("Ramp", `${s.hz_per_sec} Hz/s`)}
          {row("Accel per Hz", `${s.accel_per_hz} mm/s²`)}
          {s.sweeping_accel !== null && row("Sweeping accel", `${s.sweeping_accel} mm/s²`)}
          {s.sweeping_period !== null && row("Sweeping period", `${s.sweeping_period} s`)}
        </Fields>
      )}
      {analysis && (
        <Fields aria-label="How it was analysed">
          {fit && row("Shapers Tried", fit.shapers.join(", "))}
          {fit && row("Analysed Up To", `${fit.max_freq} Hz`)}
          {row("Analysis Version", analysis.core_version)}
        </Fields>
      )}
      {meta.notes.length > 0 && (
        <ul className="list-disc pl-5 text-sm">
          {meta.notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}
      <Table>
        <caption className="sr-only">Chips</caption>
        <TableHeader>
          <TableRow>
            <TableHead>Chip</TableHead>
            <TableHead>Type</TableHead>
            <TableHead>Axes map</TableHead>
            <TableHead>Rate (nominal / observed)</TableHead>
            <TableHead>Health</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {meta.chips.map((c) => (
            <TableRow key={c.name} className="align-top">
              <TableCell className="font-mono">{c.name}</TableCell>
              <TableCell>{c.chip_type ?? "–"}</TableCell>
              <TableCell className="font-mono">{c.axes_map ?? "–"}</TableCell>
              <TableCell className="font-mono">
                {c.nominal_rate_hz ?? "–"} / {c.observed_rate_hz?.toFixed(0) ?? "–"} Hz
              </TableCell>
              <TableCell className="whitespace-normal">
                {c.health}
                {c.health_notes.length > 0 && (
                  <span className="text-muted-foreground"> ({c.health_notes.join("; ")})</span>
                )}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
      {degraded.length > 0 && (
        <div className="space-y-1 text-sm">
          <h3 className="text-xs uppercase tracking-wide text-muted-foreground">Degraded segments</h3>
          <ul className="list-disc pl-5 text-muted-foreground">
            {degraded.map((d) => (
              <li key={d.key}>
                {d.where}: {d.chip}
                {d.notes.length > 0 ? `, ${d.notes.join("; ")}` : ""}
              </li>
            ))}
          </ul>
        </div>
      )}
    </Panel>
  );
}
