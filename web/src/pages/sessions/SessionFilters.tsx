import { ListFilterIcon } from "lucide-react";
import type { DateRange } from "react-day-picker";

import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
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
  return (
    <Popover>
      <FilterTrigger label={label} active={value !== ALL} />
      <PopoverContent>
        <Choice label={label} everything={everything} value={value} options={options} onChange={change} />
      </PopoverContent>
    </Popover>
  );
}

function FilterTrigger({ label, active }: { label: string; active: boolean }) {
  return (
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
  );
}

function parseDay(day: string | undefined): Date | undefined {
  const match = day ? /^(\d{4})-(\d{2})-(\d{2})$/.exec(day) : null;
  return match ? new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3])) : undefined;
}

function formatDay(date: Date | undefined): string | undefined {
  if (!date) return undefined;
  const two = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())}`;
}

export function DateFilter({ filters, onChange }: Pick<Props, "filters" | "onChange">) {
  const from = parseDay(filters.createdFrom);
  const to = parseDay(filters.createdTo);
  const active = Boolean(from || to);
  const choose = (range: DateRange | undefined) =>
    onChange({ ...filters, createdFrom: formatDay(range?.from), createdTo: formatDay(range?.to) });
  return (
    <Popover>
      <FilterTrigger label="Created" active={active} />
      <PopoverContent className="w-auto space-y-2 p-0">
        <Calendar mode="range" selected={from ? { from, to } : undefined} onSelect={choose} defaultMonth={from} />
        {active && (
          <div className="flex items-center justify-between gap-2 border-t px-3 py-2 text-xs text-muted-foreground">
            <span>
              {filters.createdFrom ?? "…"} to {filters.createdTo ?? filters.createdFrom ?? "…"}
            </span>
            <Button type="button" variant="outline" size="xs" onClick={() => choose(undefined)}>
              Clear dates
            </Button>
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
