import { useState } from "react";

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { Axis } from "@/lib/generated/Axis";

import type { ImportEntry } from "./importModel";

/** The select entry that reveals the typed chip field. */
const OTHER = "__other__";
const SELECT_CLASS = "h-8 w-full rounded-lg border border-input bg-transparent px-2 text-sm";

interface Props {
  entry: ImportEntry;
  known: string[];
  onEdit: (patch: Partial<Pick<ImportEntry, "chip" | "axis" | "point">>) => void;
}

/** The fields of one file. With known chips the chip is picked or typed after "Other…"; without them it is typed. */
export function ImportFileEditor({ entry, known, onEdit }: Props) {
  const [other, setOther] = useState(entry.chip !== "" && !known.includes(entry.chip));
  const typing = known.length === 0 || other;
  return (
    <div className="w-64 space-y-3">
      {known.length > 0 && (
        <div className="space-y-1">
          <Label htmlFor={`chip-${entry.id}`}>Chip</Label>
          <select
            id={`chip-${entry.id}`}
            className={SELECT_CLASS}
            value={other ? OTHER : entry.chip}
            onChange={(e) => {
              const next = e.target.value;
              setOther(next === OTHER);
              if (next !== OTHER) onEdit({ chip: next });
            }}
          >
            <option value="">Choose a chip</option>
            {known.map((section) => (
              <option key={section} value={section}>
                {section}
              </option>
            ))}
            <option value={OTHER}>Other…</option>
          </select>
        </div>
      )}
      {typing && (
        <div className="space-y-1">
          <Label htmlFor={`chip-name-${entry.id}`}>Chip name</Label>
          <Input
            id={`chip-name-${entry.id}`}
            placeholder="adxl345"
            value={entry.chip}
            onChange={(e) => onEdit({ chip: e.target.value })}
          />
        </div>
      )}
      <div className="space-y-1">
        <Label htmlFor={`axis-${entry.id}`}>Axis</Label>
        <select
          id={`axis-${entry.id}`}
          className={SELECT_CLASS}
          value={entry.axis}
          onChange={(e) => onEdit({ axis: e.target.value as Axis | "" })}
        >
          <option value="">Choose an axis</option>
          <option value="X">X</option>
          <option value="Y">Y</option>
        </select>
      </div>
      <div className="space-y-1">
        <Label htmlFor={`point-${entry.id}`}>Point</Label>
        <Input
          id={`point-${entry.id}`}
          placeholder="x, y, z in mm (optional)"
          value={entry.point}
          onChange={(e) => onEdit({ point: e.target.value })}
        />
      </div>
    </div>
  );
}
