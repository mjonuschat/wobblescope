import type { ReactNode } from "react";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useChips, useSettings } from "@/lib/api/queries";
import type { AxisLimits } from "@/lib/generated/AxisLimits";
import type { Chip } from "@/lib/generated/Chip";
import type { ChipList } from "@/lib/generated/ChipList";
import type { PrinterSettings } from "@/lib/generated/PrinterSettings";
import type { Setting } from "@/lib/generated/Setting";

const UNKNOWN = "unknown";

/** A number with its unit, or "unknown" when Klipper did not report it. */
export function measure(value: number | null | undefined, unit: string, digits = 0): string {
  if (value === null || value === undefined) return UNKNOWN;
  const text = value.toLocaleString("en-US", { maximumFractionDigits: digits, useGrouping: false });
  return unit ? `${text} ${unit}` : text;
}

function Field({ label, className, children }: { label: string; className?: string; children: ReactNode }) {
  return (
    <div className={className}>
      <dt className="text-xs uppercase tracking-wide text-muted-foreground">{label}</dt>
      <dd className="font-mono text-sm">{children}</dd>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{title}</CardTitle>
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}

function Fields({ children }: { children: ReactNode }) {
  return <dl className="grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-3 lg:grid-cols-4">{children}</dl>;
}

/** The firmware's display name: Klipper for mainline, Kalico for its fork. */
export function firmwareName(variant: PrinterSettings["variant"]): string {
  return { mainline: "Klipper", kalico: "Kalico", unknown: UNKNOWN }[variant];
}

function MachineSection({ settings }: { settings: PrinterSettings }) {
  const m = settings.machine;
  return (
    <Section title={m.name ?? "Printer"}>
      <Fields>
        <Field label="Kinematics">{m.kinematics ?? UNKNOWN}</Field>
        <Field label="Firmware" className="sm:col-span-2 lg:col-span-3">
          {firmwareName(settings.variant)} {m.klipper_version ?? ""}
        </Field>
        <Field label="MCUs" className="col-span-full">
          {m.mcus.length > 0 ? m.mcus.map((name) => name.replace(/^mcu /, "")).join(", ") : UNKNOWN}
        </Field>
      </Fields>
    </Section>
  );
}

function MotionSection({ settings }: { settings: PrinterSettings }) {
  const m = settings.machine;
  return (
    <Section title="Motion limits">
      <Fields>
        <Field label="Max velocity">{measure(m.max_velocity, "mm/s")}</Field>
        <Field label="Max acceleration">{measure(m.max_accel, "mm/s²")}</Field>
        <Field label="Square corner velocity">{measure(m.square_corner_velocity, "mm/s", 1)}</Field>
        <Field label="Minimum cruise ratio">{measure(m.minimum_cruise_ratio, "", 2)}</Field>
        <Field label="Max Z velocity">{measure(m.max_z_velocity, "mm/s")}</Field>
        <Field label="Max Z acceleration">{measure(m.max_z_accel, "mm/s²")}</Field>
      </Fields>
    </Section>
  );
}

function travel(limits: AxisLimits | null, axis: "x" | "y" | "z"): string {
  if (!limits) return UNKNOWN;
  const { min, max } = limits[axis];
  return `${measure(min, "", 1)} to ${measure(max, "mm", 1)} (${measure(max - min, "mm", 1)})`;
}

function VolumeSection({ settings }: { settings: PrinterSettings }) {
  const limits = settings.axis_limits;
  return (
    <Section title="Build volume">
      <Fields>
        <Field label="X travel">{travel(limits, "x")}</Field>
        <Field label="Y travel">{travel(limits, "y")}</Field>
        <Field label="Z travel">{travel(limits, "z")}</Field>
        <Field label="Default test point">
          {settings.default_point ? settings.default_point.map((v) => measure(v, "", 1)).join(", ") : UNKNOWN}
        </Field>
      </Fields>
    </Section>
  );
}

function setting(s: Setting, unit: string, digits = 0): string {
  return measure(s.value, unit, digits);
}

function ResonanceSection({ settings }: { settings: PrinterSettings }) {
  const s = settings.settings;
  const sweep = settings.default_mode === "Sweep";
  return (
    <Section title="Resonance testing">
      <div className="space-y-4">
        <Fields>
          <Field label="Default mode">{sweep ? "Sweep" : "Pulse"}</Field>
          {sweep && (
            <>
              <Field label="Sweeping acceleration">{setting(s.sweeping_accel, "mm/s²")}</Field>
              <Field label="Sweeping period">{setting(s.sweeping_period, "s", 1)}</Field>
            </>
          )}
        </Fields>
        <Fields>
          <Field label="Frequency range">
            {s.freq_start.value === null || s.freq_end.value === null
              ? UNKNOWN
              : `${setting(s.freq_start, "")} to ${setting(s.freq_end, "Hz")}`}
          </Field>
          <Field label="Hz per second">{setting(s.hz_per_sec, "Hz/s", 1)}</Field>
          <Field label="Acceleration per Hz">{setting(s.accel_per_hz, "mm/s² per Hz", 1)}</Field>
          <Field label="Move speed">{setting(s.move_speed, "mm/s")}</Field>
        </Fields>
      </div>
    </Section>
  );
}

function defaultAxes(chips: ChipList, chip: Chip): string {
  return (["x", "y"] as const)
    .filter((axis) => chips.defaults[axis].includes(chip.section))
    .map((axis) => axis.toUpperCase())
    .join(", ");
}

function ChipsSection({ chips }: { chips: ChipList }) {
  return (
    <Section title="Accelerometers">
      {chips.chips.length === 0 ? (
        <p className="text-sm text-muted-foreground">No accelerometers are configured.</p>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Chip</TableHead>
              <TableHead>Type</TableHead>
              <TableHead>MCU</TableHead>
              <TableHead>Rate</TableHead>
              <TableHead>Default for</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {chips.chips.map((chip) => {
              const axes = defaultAxes(chips, chip);
              return (
                <TableRow key={chip.section}>
                  <TableCell className="font-mono">{chip.section}</TableCell>
                  <TableCell>{chip.chip_type}</TableCell>
                  <TableCell>{chip.mcu ?? UNKNOWN}</TableCell>
                  <TableCell className="font-mono">{measure(chip.nominal_rate_hz, "Hz")}</TableCell>
                  <TableCell>{axes ? <Badge variant="secondary">{axes}</Badge> : "none"}</TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      )}
    </Section>
  );
}

/** What the printer is, how it moves, and how Klipper is set up to test it. */
export function PrinterPage() {
  const settings = useSettings();
  const chips = useChips();
  return (
    <div className="mx-auto max-w-5xl space-y-4">
      <h1 className="text-xl font-semibold">Printer</h1>
      {settings.isPending && <p className="text-sm text-muted-foreground">Reading the printer's settings…</p>}
      {settings.isError && (
        <p role="alert" className="text-sm text-destructive">
          {settings.error.message}
        </p>
      )}
      {settings.data && (
        <>
          <MachineSection settings={settings.data} />
          <MotionSection settings={settings.data} />
          <VolumeSection settings={settings.data} />
          <ResonanceSection settings={settings.data} />
        </>
      )}
      {chips.data && <ChipsSection chips={chips.data} />}
    </div>
  );
}
