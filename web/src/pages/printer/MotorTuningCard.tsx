import { ChevronDownIcon } from "lucide-react";
import { Fragment, useState } from "react";

import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { MotorTuning } from "@/lib/generated/MotorTuning";
import { CardLabel } from "@/ui-kit/CardLabel";
import { Field, Fields } from "@/ui-kit/Fields";

/** The options listed under a row, spelled as Klipper spells them. */
const OPTIONS = [
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

const NONE = "–";

function show(value: number | string | null): string {
  return value === null ? NONE : String(value);
}

function Row({ entry }: { entry: MotorTuning }) {
  const [open, setOpen] = useState(false);
  const set = OPTIONS.filter((key) => entry[key] !== null);
  return (
    <Fragment>
      <TableRow>
        <TableCell>{entry.stepper}</TableCell>
        <TableCell>{show(entry.driver)}</TableCell>
        <TableCell>{show(entry.motor)}</TableCell>
        <TableCell>{show(entry.tuning_goal)}</TableCell>
        <TableCell>{entry.voltage === null ? NONE : `${entry.voltage} V`}</TableCell>
        <TableCell className="text-right">
          <button
            type="button"
            aria-expanded={open}
            aria-label={`${open ? "Hide" : "Show"} all values for ${entry.stepper}`}
            onClick={() => setOpen(!open)}
            className="rounded p-0.5 text-muted-foreground hover:text-foreground"
          >
            <ChevronDownIcon aria-hidden="true" className={`size-4 transition-transform ${open ? "rotate-180" : ""}`} />
          </button>
        </TableCell>
      </TableRow>
      {open && (
        <TableRow>
          <TableCell colSpan={6}>
            {set.length === 0 ? (
              <p className="text-sm text-muted-foreground">No other values are set.</p>
            ) : (
              <Fields>
                {set.map((key) => (
                  <Field key={key} label={key}>
                    {show(entry[key])}
                  </Field>
                ))}
              </Fields>
            )}
          </TableCell>
        </TableRow>
      )}
    </Fragment>
  );
}

export function MotorTuningCard({ tuning }: { tuning: MotorTuning[] }) {
  if (tuning.length === 0) return null;
  return (
    <Card>
      <CardHeader>
        <CardLabel>Motor tuning</CardLabel>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Stepper</TableHead>
              <TableHead>Driver</TableHead>
              <TableHead>Motor</TableHead>
              <TableHead>Tuning goal</TableHead>
              <TableHead>Voltage</TableHead>
              <TableHead />
            </TableRow>
          </TableHeader>
          <TableBody>
            {tuning.map((entry) => (
              <Row key={entry.stepper} entry={entry} />
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}
