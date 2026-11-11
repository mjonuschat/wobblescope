import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

/** The range choice. It is a fixed top frequency, or the most each chip can show. */
export type Range = number | "max";

const WINDOW_SECONDS = [0.5, 1, 2, 4];
const RANGES_HZ = [100, 200, 400, 800];
const RATES = [1, 2, 5, 10, 20];

/** The update rates up to the daemon's limit, which is the top choice. */
function rateOptions(limit: number, current: number): number[] {
  return [...new Set([...RATES.filter((r) => r < limit), limit, current])].sort((a, b) => a - b);
}

/** The fixed ranges, with the daemon's default when it is not one of them. */
function rangeOptions(current: number): number[] {
  return [...new Set([...RANGES_HZ, current])].sort((a, b) => a - b);
}

interface Props {
  updatesPerSecond: number;
  rateLimit: number;
  onUpdatesPerSecond: (value: number) => void;
  range: Range;
  /** The fixed range the daemon starts with. */
  defaultMaxHz: number;
  onRange: (value: Range) => void;
  windowSeconds: number;
  onWindowSeconds: (value: number) => void;
  paused: boolean;
  onPaused: (value: boolean) => void;
  onResetPeakHold: () => void;
}

function Choice({
  label,
  value,
  text,
  onChange,
  children,
}: {
  label: string;
  value: string;
  text: string;
  onChange: (value: string) => void;
  children: ReactNode;
}) {
  const id = `display-${label.toLowerCase().replaceAll(" ", "-")}`;
  return (
    <div className="space-y-2">
      <Label htmlFor={id}>{label}</Label>
      <Select value={value} onValueChange={(next) => onChange(String(next))}>
        <SelectTrigger id={id} className="w-40">
          <SelectValue>{text}</SelectValue>
        </SelectTrigger>
        <SelectContent>{children}</SelectContent>
      </Select>
    </div>
  );
}

/** The display choices: refresh rate, range, spectrum window, pause and peak-hold reset. */
export function DisplaySettings(props: Props) {
  const rateText = (r: number) => `${r} Hz`;
  const rangeText = (range: Range) => (range === "max" ? "Max" : `${range} Hz`);
  return (
    <div className="flex flex-wrap items-end gap-4">
      <Choice
        label="Refresh rate"
        value={String(props.updatesPerSecond)}
        text={rateText(props.updatesPerSecond)}
        onChange={(value) => props.onUpdatesPerSecond(Number(value))}
      >
        {rateOptions(props.rateLimit, props.updatesPerSecond).map((r) => (
          <SelectItem key={r} value={String(r)}>
            {rateText(r)}
          </SelectItem>
        ))}
      </Choice>
      <Choice
        label="Frequency range"
        value={String(props.range)}
        text={rangeText(props.range)}
        onChange={(value) => props.onRange(value === "max" ? "max" : Number(value))}
      >
        {rangeOptions(props.defaultMaxHz).map((hz) => (
          <SelectItem key={hz} value={String(hz)}>
            {rangeText(hz)}
          </SelectItem>
        ))}
        <SelectItem value="max">{rangeText("max")}</SelectItem>
      </Choice>
      <Choice
        label="Spectrum window"
        value={String(props.windowSeconds)}
        text={`${props.windowSeconds} s`}
        onChange={(value) => props.onWindowSeconds(Number(value))}
      >
        {WINDOW_SECONDS.map((s) => (
          <SelectItem key={s} value={String(s)}>
            {s} s
          </SelectItem>
        ))}
      </Choice>
      <Button type="button" variant="outline" aria-pressed={props.paused} onClick={() => props.onPaused(!props.paused)}>
        {props.paused ? "Resume" : "Pause"}
      </Button>
      <Button type="button" variant="outline" onClick={props.onResetPeakHold}>
        Reset peak hold
      </Button>
    </div>
  );
}
