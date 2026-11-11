import { Button } from "@/components/ui/button";

/** The range choice. It is a fixed top frequency or the most the chips allow. */
export type Range = number | "full";

const WINDOW_SECONDS = [0.5, 1, 2, 4];
const RANGES_HZ = [100, 200, 400];
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
  /** What "full" means now, in Hz. */
  fullHz: number;
  onRange: (value: Range) => void;
  windowSeconds: number;
  onWindowSeconds: (value: number) => void;
  paused: boolean;
  onPaused: (value: boolean) => void;
  onResetPeakHold: () => void;
}

/** The display choices: update rate, range, spectrum window, pause and peak-hold reset. */
export function DisplaySettings(props: Props) {
  const select = "rounded-md border border-neutral-700 bg-neutral-900 px-2 py-1 text-sm";
  return (
    <div className="flex flex-wrap items-end gap-4">
      <label className="space-y-1 text-sm">
        <span className="block">Update rate</span>
        <select
          className={select}
          value={props.updatesPerSecond}
          onChange={(e) => props.onUpdatesPerSecond(Number(e.target.value))}
        >
          {rateOptions(props.rateLimit, props.updatesPerSecond).map((r) => (
            <option key={r} value={r}>
              {r} per second
            </option>
          ))}
        </select>
      </label>
      <label className="space-y-1 text-sm">
        <span className="block">Frequency range</span>
        <select
          className={select}
          value={String(props.range)}
          onChange={(e) => props.onRange(e.target.value === "full" ? "full" : Number(e.target.value))}
        >
          {rangeOptions(props.defaultMaxHz).map((hz) => (
            <option key={hz} value={hz}>
              {hz} Hz
            </option>
          ))}
          <option value="full">Full ({props.fullHz} Hz)</option>
        </select>
      </label>
      <label className="space-y-1 text-sm">
        <span className="block">Spectrum window</span>
        <select
          className={select}
          value={props.windowSeconds}
          onChange={(e) => props.onWindowSeconds(Number(e.target.value))}
        >
          {WINDOW_SECONDS.map((s) => (
            <option key={s} value={s}>
              {s} s
            </option>
          ))}
        </select>
      </label>
      <Button type="button" variant="outline" aria-pressed={props.paused} onClick={() => props.onPaused(!props.paused)}>
        {props.paused ? "Resume" : "Pause"}
      </Button>
      <Button type="button" variant="outline" onClick={props.onResetPeakHold}>
        Reset peak hold
      </Button>
    </div>
  );
}
