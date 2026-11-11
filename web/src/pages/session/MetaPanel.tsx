import { Field, Fields } from "@/components/Fields";
import { Panel } from "@/components/Panel";
import { pointLabel } from "@/components/charts/options";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { formatTime } from "@/lib/format";
import type { Outcome } from "@/lib/generated/Outcome";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

function outcomeText(outcome: Outcome): string {
  if (outcome === "Complete") return "Complete";
  if ("Incomplete" in outcome) return `Incomplete: ${outcome.Incomplete.reason}`;
  return `Failed: ${outcome.Failed.reason}`;
}

function sourceText(meta: SessionMeta): string {
  if (meta.source === "Capture") return "Capture";
  const name = meta.source.CsvImport.original_file_name;
  return name ? `CSV import of ${name}` : "CSV import";
}

const row = (label: string, value: React.ReactNode) => (
  <Field key={label} label={label}>
    {value}
  </Field>
);

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

export function MetaPanel({ meta }: { meta: SessionMeta }) {
  const s = meta.settings;
  const degraded = degradedSegments(meta);
  return (
    <Panel title="Session" label="Session details">
      <Fields>
        {row("Source", sourceText(meta))}
        {row("Created", formatTime(meta.created_at))}
        {row("Mode", meta.mode ?? "Not recorded")}
        {row("Outcome", outcomeText(meta.outcome))}
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
        {row("Probe points", meta.probe_points.map((_, i) => pointLabel(i, meta.probe_points)).join(", "))}
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
