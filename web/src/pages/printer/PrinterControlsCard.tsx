import { useIsMutating } from "@tanstack/react-query";
import { ListChecksIcon, ScaleIcon } from "lucide-react";

import { HomeSolidIcon } from "@/components/HomeSolidIcon";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import {
  homeMutationKey,
  levelMutationKey,
  prepareMutationKey,
  useHomePrinter,
  useLevelPrinter,
  usePreparePrinter,
  usePrinterState,
  useStatus,
} from "@/lib/api/queries";
import type { HomeAxes } from "@/lib/generated/HomeAxes";
import type { LevelKind } from "@/lib/generated/LevelKind";
import { CardLabel } from "@/ui-kit/CardLabel";
import { StatusBadge } from "@/ui-kit/StatusBadge";

/** The text beside the house icon, and the name a screen reader gets. */
const HOME_BUTTONS: { axes: HomeAxes; label: string; name: string }[] = [
  { axes: "xy", label: "XY", name: "Home XY" },
  { axes: "x", label: "X", name: "Home X" },
  { axes: "y", label: "Y", name: "Home Y" },
  { axes: "z", label: "Z", name: "Home Z" },
  { axes: "all", label: "ALL", name: "Home all" },
];

/** The text beside the scale icon, and the name a screen reader gets, for each kind of levelling. */
const LEVEL_BUTTON: Record<LevelKind, { label: string; name: string }> = {
  z_tilt: { label: "Z-TILT", name: "Z-tilt adjust" },
  quad_gantry_level: { label: "QGL", name: "Quad gantry level" },
};

export function PrinterControlsCard() {
  const status = useStatus();
  const ready = status.data?.moonraker.state === "connected" && status.data.klippy.state === "ready";
  const printer = usePrinterState(ready);
  const home = useHomePrinter();
  const level = useLevelPrinter();
  const prepare = usePreparePrinter();
  const preparing = useIsMutating({ mutationKey: prepareMutationKey }) > 0;
  const homing = useIsMutating({ mutationKey: homeMutationKey }) > 0;
  const levelling = useIsMutating({ mutationKey: levelMutationKey }) > 0;

  const homed = (printer.data?.homed_axes ?? "").toLowerCase();
  const levelKind = printer.data?.level_kind ?? null;
  const busyReason = printer.data?.busy ?? null;
  const moving = homing || levelling || preparing;
  const blocked = !ready || moving || busyReason !== null;

  return (
    <Card>
      <CardHeader>
        <CardLabel>Homing and levelling</CardLabel>
      </CardHeader>
      <CardContent className="space-y-4">
        <ul aria-label="Printer status" className="flex flex-wrap items-center gap-x-5 gap-y-1">
          {(["x", "y", "z"] as const).map((axis) => (
            <li key={axis}>
              <StatusBadge tone={ready && homed.includes(axis) ? "success" : undefined}>
                {axis.toUpperCase()} {ready && homed.includes(axis) ? "homed" : "not homed"}
              </StatusBadge>
            </li>
          ))}
          {levelKind && (
            <li>
              <StatusBadge tone={ready && printer.data?.levelled ? "success" : undefined}>
                {ready && printer.data?.levelled ? "Levelled" : "Not levelled"}
              </StatusBadge>
            </li>
          )}
        </ul>
        <div className="flex flex-wrap gap-2">
          {levelKind && (
            <Button
              type="button"
              aria-label="Prepare the printer"
              disabled={blocked}
              title={`Runs G28, ${printer.data?.level_command ?? "the levelling"}, then G28 Z`}
              onClick={() => prepare.mutate()}
            >
              <ListChecksIcon data-icon="inline-start" />
              PREP
            </Button>
          )}
          {HOME_BUTTONS.map(({ axes, label, name }) => (
            <Button
              key={axes}
              type="button"
              variant="outline"
              aria-label={name}
              disabled={blocked}
              onClick={() => home.mutate(axes)}
            >
              <HomeSolidIcon data-icon="inline-start" />
              {label}
            </Button>
          ))}
          {levelKind && (
            <Button
              type="button"
              variant="outline"
              aria-label={LEVEL_BUTTON[levelKind].name}
              disabled={blocked}
              title={printer.data?.level_command ? `Runs ${printer.data.level_command}` : undefined}
              onClick={() => level.mutate()}
            >
              <ScaleIcon data-icon="inline-start" />
              {LEVEL_BUTTON[levelKind].label}
            </Button>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
