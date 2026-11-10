import { pointLabel } from "@/components/charts/options";
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
  <div key={label}>
    <dt className="text-muted-foreground">{label}</dt>
    <dd>{value}</dd>
  </div>
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
    <section className="space-y-4 rounded-lg border p-4" aria-label="Session details">
      <dl className="grid grid-cols-2 gap-x-6 gap-y-2 text-sm sm:grid-cols-4">
        {row("Source", sourceText(meta))}
        {row("Created", formatTime(meta.created_at))}
        {row("Mode", meta.mode ?? "Not recorded")}
        {row("Outcome", outcomeText(meta.outcome))}
        {row("Printer", meta.printer?.name ?? "Not recorded")}
        {meta.printer?.klipper_version && row("Klipper", meta.printer.klipper_version)}
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
      </dl>
      {s && (
        <dl className="grid grid-cols-2 gap-x-6 gap-y-2 text-sm sm:grid-cols-4" aria-label="Recorded sweep settings">
          {row("Frequency range", `${s.freq_start} to ${s.freq_end} Hz`)}
          {row("Ramp", `${s.hz_per_sec} Hz/s`)}
          {row("Accel per Hz", `${s.accel_per_hz} mm/s²`)}
          {s.sweeping_accel !== null && row("Sweeping accel", `${s.sweeping_accel} mm/s²`)}
          {s.sweeping_period !== null && row("Sweeping period", `${s.sweeping_period} s`)}
        </dl>
      )}
      {meta.notes.length > 0 && (
        <ul className="list-disc pl-5 text-sm">
          {meta.notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}
      <table className="w-full text-sm">
        <caption className="sr-only">Chips</caption>
        <thead>
          <tr className="text-left text-muted-foreground">
            <th>Chip</th>
            <th>Type</th>
            <th>Axes map</th>
            <th>Rate (nominal / observed)</th>
            <th>Health</th>
          </tr>
        </thead>
        <tbody>
          {meta.chips.map((c) => (
            <tr key={c.name} className="align-top">
              <td>{c.name}</td>
              <td>{c.chip_type ?? "–"}</td>
              <td>{c.axes_map ?? "–"}</td>
              <td>
                {c.nominal_rate_hz ?? "–"} / {c.observed_rate_hz?.toFixed(0) ?? "–"} Hz
              </td>
              <td>
                {c.health}
                {c.health_notes.length > 0 && (
                  <span className="text-muted-foreground"> ({c.health_notes.join("; ")})</span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {degraded.length > 0 && (
        <div className="space-y-1 text-sm">
          <h3 className="font-medium">Degraded segments</h3>
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
    </section>
  );
}
