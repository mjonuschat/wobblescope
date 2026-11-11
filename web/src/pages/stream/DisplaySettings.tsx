import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { PauseIcon, PlayIcon } from "lucide-react";

import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

/** The range choice. It is a fixed top frequency, or the most each chip can show. */
export type Range = number | "max";

const WINDOWS = [
  { seconds: 0.5, name: "Fast" },
  { seconds: 1, name: "Balanced" },
  { seconds: 2, name: "Detailed" },
  { seconds: 4, name: "Very Detailed" },
];
const RANGES_HZ = [100, 200, 400, 800];
const RATES = [1, 2, 5, 10, 20];

/** The update rates up to the daemon's limit, which is the top choice. */
function rateOptions(limit: number, current: number): number[] {
  return [...new Set([...RATES.filter((r) => r < limit), limit, current])].sort((a, b) => a - b);
}

function windowText(seconds: number): string {
  const name = WINDOWS.find((w) => w.seconds === seconds)?.name;
  return name ?? `${seconds} s`;
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
        label="Refresh Rate"
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
        label="Frequency Range"
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
        label="Averaging Time"
        value={String(props.windowSeconds)}
        text={windowText(props.windowSeconds)}
        onChange={(value) => props.onWindowSeconds(Number(value))}
      >
        {WINDOWS.map(({ seconds }) => (
          <SelectItem key={seconds} value={String(seconds)}>
            {windowText(seconds)}
          </SelectItem>
        ))}
      </Choice>
    </div>
  );
}

const PAUSE_COLOURS =
  "border-orange-500/40 bg-orange-500/10 text-orange-400 hover:bg-orange-500/20 hover:text-orange-300";
const RESUME_COLOURS = "border-green-500/40 bg-green-500/10 text-green-400 hover:bg-green-500/20 hover:text-green-300";

/** Freezes the charts, or lets them run again. The connection stays open either way. */
export function PauseButton({ paused, onPaused }: { paused: boolean; onPaused: (value: boolean) => void }) {
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      aria-pressed={paused}
      className={paused ? RESUME_COLOURS : PAUSE_COLOURS}
      onClick={() => onPaused(!paused)}
    >
      {paused ? <PlayIcon /> : <PauseIcon />}
      {paused ? "Resume" : "Pause"}
    </Button>
  );
}
