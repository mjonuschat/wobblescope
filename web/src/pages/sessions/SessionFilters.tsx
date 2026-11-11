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
      <SelectTrigger aria-label={label} className="min-w-40">
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

export function SessionFilters({ filters, onChange, all }: Props) {
  const chips = [...new Set(all.flatMap((s) => s.chips))].sort();
  const printers = [...new Set(all.flatMap((s) => (s.printer ? [s.printer] : [])))].sort();
  const same = (names: string[]) => names.map((name) => ({ value: name, label: name }));
  return (
    <div className="flex flex-wrap gap-3">
      <Choice
        label="Chip"
        everything="All Chips"
        value={filters.chip ?? ALL}
        options={same(chips)}
        onChange={(value) => onChange({ ...filters, chip: value === ALL ? undefined : value })}
      />
      <Choice
        label="Printer"
        everything="All Printers"
        value={filters.printer ?? ALL}
        options={same(printers)}
        onChange={(value) => onChange({ ...filters, printer: value === ALL ? undefined : value })}
      />
      <Choice
        label="Pinned"
        everything="All Sessions"
        value={filters.pinned === undefined ? ALL : String(filters.pinned)}
        options={[
          { value: "true", label: "Pinned Only" },
          { value: "false", label: "Unpinned Only" },
        ]}
        onChange={(value) => onChange({ ...filters, pinned: value === ALL ? undefined : value === "true" })}
      />
    </div>
  );
}
