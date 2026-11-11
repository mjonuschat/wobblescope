import { ListFilterIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import type { SessionFilters as Filters } from "@/lib/api/endpoints";
import type { Summary } from "@/lib/generated/Summary";

/** The value of a filter's "everything" choice; a select cannot hold an empty one. */
const ALL = "all";

interface Props {
  filters: Filters;
  onChange: (filters: Filters) => void;
  /** The unfiltered list, which supplies the chip and printer choices. */
  all: Summary[];
}

/** Which column a filter belongs to. */
export type FilterColumn = "chip" | "printer" | "pinned";

function Choice({
  label,
  everything,
  value,
  options,
  onChange,
}: {
  label: string;
  everything: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
}) {
  const items = [{ value: ALL, label: everything }, ...options];
  return (
    <Select items={items} value={value} onValueChange={(next) => onChange(next as string)}>
      <SelectTrigger aria-label={label} className="w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {items.map((item) => (
          <SelectItem key={item.value} value={item.value}>
            {item.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

const same = (names: string[]) => names.map((name) => ({ value: name, label: name }));

/**
 * A filter icon for a column header. It opens a popover with a select box, and is outlined in
 * the accent colour while the column is filtered.
 */
export function ColumnFilter({ column, filters, onChange, all }: Props & { column: FilterColumn }) {
  let label: string;
  let everything: string;
  let value: string;
  let options: { value: string; label: string }[];
  let change: (value: string) => void;
  if (column === "chip") {
    label = "Chip";
    everything = "All Chips";
    value = filters.chip ?? ALL;
    options = same([...new Set(all.flatMap((s) => s.chips))].sort());
    change = (next) => onChange({ ...filters, chip: next === ALL ? undefined : next });
  } else if (column === "printer") {
    label = "Printer";
    everything = "All Printers";
    value = filters.printer ?? ALL;
    options = same([...new Set(all.flatMap((s) => (s.printer ? [s.printer] : [])))].sort());
    change = (next) => onChange({ ...filters, printer: next === ALL ? undefined : next });
  } else {
    label = "Pinned";
    everything = "All Sessions";
    value = filters.pinned === undefined ? ALL : String(filters.pinned);
    options = [
      { value: "true", label: "Pinned Only" },
      { value: "false", label: "Unpinned Only" },
    ];
    change = (next) => onChange({ ...filters, pinned: next === ALL ? undefined : next === "true" });
  }
  const active = value !== ALL;
  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={`Filter ${label}`}
            className={active ? "bg-primary/15 text-primary hover:bg-primary/25" : "text-muted-foreground"}
          />
        }
      >
        <ListFilterIcon />
      </PopoverTrigger>
      <PopoverContent>
        <Choice label={label} everything={everything} value={value} options={options} onChange={change} />
      </PopoverContent>
    </Popover>
  );
}
