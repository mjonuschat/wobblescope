import { ChevronDownIcon } from "lucide-react";
import { Fragment, useState } from "react";

import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useMotorRegisters } from "@/lib/api/queries";
import type { MotorTuning } from "@/lib/generated/MotorTuning";
import { cn } from "@/lib/utils";
import { Alert } from "@/ui-kit/Alert";
import { CardLabel } from "@/ui-kit/CardLabel";
import { Field, Fields } from "@/ui-kit/Fields";

/** The autotune values listed under a row, spelled as Klipper spells them. */
const AUTOTUNE = [
  "voltage",
  "extra_hysteresis",
  "tbl",
  "toff",
  "tpfd",
  "sgt",
  "sg4_thrs",
  "semin",
  "semax",
  "seup",
  "sedn",
  "seimin",
  "overvoltage_vth",
  "pwm_freq_target",
] as const;

type Key = keyof MotorTuning;

const NONE = "–";

/** A tint on what differs from the other motor. Its meaning is also in the title and the text. */
const DIFFERS = "bg-warning/10";

function show(value: number | string | null): string {
  return value === null ? NONE : String(value);
}

function amperes(value: number | null): string {
  return value === null ? NONE : value.toFixed(2);
}

function shown(entry: MotorTuning, key: Key): string {
  const value = entry[key];
  if (key === "run_current" || key === "hold_current") return amperes(value as number | null);
  return show(value as number | string | null);
}

function Registers({ query }: { query: ReturnType<typeof useMotorRegisters> }) {
  if (query.isPending) return <p className="text-sm text-muted-foreground">Reading registers…</p>;
  if (query.isError) return <p className="text-sm text-muted-foreground">{query.error.message}</p>;
  const { registers, differs, differs_from: other } = query.data;
  if (registers.length === 0) return <p className="text-sm text-muted-foreground">The driver reported no registers.</p>;
  return (
    <Fields>
      {registers.map((register) => {
        const difference = differs.find((d) => d.name === register.name);
        const reference = difference && other ? `${other}: ${difference.other}` : undefined;
        return (
          <Field key={register.name} label={register.name} className={reference && cn(DIFFERS, "-mx-1 rounded px-1")}>
            <span title={reference}>{register.value}</span>
            {reference && <span className="sr-only"> (differs, {reference})</span>}
          </Field>
        );
      })}
    </Fields>
  );
}

function Row({ entry, other }: { entry: MotorTuning; other: MotorTuning | undefined }) {
  const [open, setOpen] = useState(false);
  const set = AUTOTUNE.filter((key) => entry[key] !== null);
  const hasDriver = entry.driver !== null;
  const registers = useMotorRegisters(entry.stepper, open && hasDriver);
  const differs = (key: Key) => entry.differs.includes(key);
  const hiddenDifference = set.some(differs);

  const Cell = ({ k, align, children }: { k: Key; align?: boolean; children: string }) => {
    const reference = other && differs(k) ? `${other.stepper}: ${shown(other, k)}` : undefined;
    return (
      <TableCell className={cn(align && "text-right tabular-nums", reference && DIFFERS)} title={reference}>
        {children}
        {reference && <span className="sr-only"> (differs, {reference})</span>}
      </TableCell>
    );
  };

  return (
    <Fragment>
      <TableRow>
        <TableCell>{entry.stepper}</TableCell>
        <Cell k="driver">{show(entry.driver)}</Cell>
        <Cell k="motor">{show(entry.motor)}</Cell>
        <Cell k="tuning_goal">{show(entry.tuning_goal)}</Cell>
        <Cell k="run_current" align>
          {amperes(entry.run_current)}
        </Cell>
        <Cell k="hold_current" align>
          {amperes(entry.hold_current)}
        </Cell>
        <Cell k="microsteps" align>
          {show(entry.microsteps)}
        </Cell>
        <TableCell className="text-right">
          {(set.length > 0 || hasDriver) && (
            <button
              type="button"
              aria-expanded={open}
              aria-label={`${open ? "Hide" : "Show"} all values for ${entry.stepper}`}
              onClick={() => setOpen(!open)}
              className="relative rounded p-0.5 text-muted-foreground hover:text-foreground"
            >
              <ChevronDownIcon
                aria-hidden="true"
                className={`size-4 transition-transform ${open ? "rotate-180" : ""}`}
              />
              {hiddenDifference && !open && (
                <span
                  role="img"
                  aria-label="Some values differ from the other motor"
                  className="absolute -top-0.5 -right-0.5 size-1.5 rounded-full bg-warning"
                />
              )}
            </button>
          )}
        </TableCell>
      </TableRow>
      {open && (
        <TableRow>
          <TableCell colSpan={8}>
            {set.length > 0 && (
              <section>
                <h3 className="mb-2 text-sm font-medium">Autotune</h3>
                <Fields>
                  {set.map((key) => {
                    const reference = other && differs(key) ? `${other.stepper}: ${shown(other, key)}` : undefined;
                    return (
                      <Field key={key} label={key} className={reference && cn(DIFFERS, "-mx-1 rounded px-1")}>
                        <span title={reference}>{show(entry[key])}</span>
                        {reference && <span className="sr-only"> (differs, {reference})</span>}
                      </Field>
                    );
                  })}
                </Fields>
              </section>
            )}
            {hasDriver && (
              <section className={cn(set.length > 0 && "mt-6")}>
                <h3 className="mb-2 text-sm font-medium">Registers</h3>
                <Registers query={registers} />
              </section>
            )}
          </TableCell>
        </TableRow>
      )}
    </Fragment>
  );
}

export function MotorTuningCard({ tuning }: { tuning: MotorTuning[] }) {
  if (tuning.length === 0) return null;
  const differing = tuning.find((entry) => entry.differs_from !== null);
  const byName = new Map(tuning.map((entry) => [entry.stepper, entry]));
  return (
    <Card>
      <CardHeader>
        <CardLabel>Motor tuning</CardLabel>
      </CardHeader>
      <CardContent>
        {differing && (
          <Alert tone="warning" variant="inline" className="mb-3">
            {differing.differs_from} and {differing.stepper} have different configurations
          </Alert>
        )}
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Stepper</TableHead>
              <TableHead>Driver</TableHead>
              <TableHead>Motor</TableHead>
              <TableHead>Tuning goal</TableHead>
              <TableHead className="text-right">Run (A)</TableHead>
              <TableHead className="text-right">Hold (A)</TableHead>
              <TableHead className="text-right">Microsteps</TableHead>
              <TableHead />
            </TableRow>
          </TableHeader>
          <TableBody>
            {tuning.map((entry) => (
              <Row
                key={entry.stepper}
                entry={entry}
                other={entry.differs_from ? byName.get(entry.differs_from) : undefined}
              />
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}
