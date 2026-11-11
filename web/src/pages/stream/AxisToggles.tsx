import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";

import type { Axis3 } from "./axes";

const CHOICES: { axis: Axis3; label: string }[] = [
  { axis: "x", label: "X" },
  { axis: "y", label: "Y" },
  { axis: "z", label: "Z" },
  { axis: "sum", label: "Total" },
];

interface Props {
  axes: Axis3[];
  onAxes: (axes: Axis3[]) => void;
}

/** Which lines the spectrum draws for every chip. At least one stays on. */
export function AxisToggles({ axes, onAxes }: Props) {
  return (
    <ToggleGroup
      multiple
      variant="outline"
      size="sm"
      aria-label="Spectrum axes"
      value={axes}
      onValueChange={(next) => {
        if (next.length > 0) onAxes(next as Axis3[]);
      }}
    >
      {CHOICES.map(({ axis, label }) => (
        <ToggleGroupItem key={axis} value={axis} aria-label={label}>
          {label}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  );
}
