import { CalendarIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";

interface Props {
  value: Date;
  onChange: (date: Date) => void;
}

const two = (n: number) => String(n).padStart(2, "0");

function timeText(date: Date): string {
  return `${two(date.getHours())}:${two(date.getMinutes())}:${two(date.getSeconds())}`;
}

/** The session's local date and time: a calendar and a time field with seconds. */
export function GroupDatePicker({ value, onChange }: Props) {
  const chooseDay = (day: Date | undefined) => {
    if (!day) return;
    onChange(
      new Date(
        day.getFullYear(),
        day.getMonth(),
        day.getDate(),
        value.getHours(),
        value.getMinutes(),
        value.getSeconds(),
      ),
    );
  };
  const chooseTime = (text: string) => {
    const [hours, minutes, seconds] = text.split(":").map(Number);
    if (Number.isNaN(hours) || Number.isNaN(minutes)) return;
    onChange(new Date(value.getFullYear(), value.getMonth(), value.getDate(), hours, minutes, seconds || 0));
  };
  return (
    <Popover>
      <PopoverTrigger render={<Button type="button" variant="outline" size="sm" aria-label="Date and time" />}>
        <CalendarIcon />
        {value.toLocaleString()}
      </PopoverTrigger>
      <PopoverContent className="w-auto space-y-2 p-0">
        <Calendar mode="single" selected={value} onSelect={chooseDay} defaultMonth={value} />
        <div className="border-t p-3">
          <Input
            type="time"
            step="1"
            aria-label="Time"
            value={timeText(value)}
            onChange={(e) => chooseTime(e.target.value)}
          />
        </div>
      </PopoverContent>
    </Popover>
  );
}
