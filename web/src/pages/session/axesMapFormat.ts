import type { AxesMapResult } from "@/lib/generated/AxesMapResult";
import type { NoiseVerdict } from "@/lib/generated/NoiseVerdict";

const NOISE_WORDS: Record<NoiseVerdict, string> = { Fine: "OK", High: "High", TooHigh: "Too high" };

/** An axes map written one way wherever it is shown: `x, -y, -z`. Klipper's own spelling is `x,-y,-z`. */
export function formatMap(map: string): string {
  return map
    .split(",")
    .map((part) => part.trim())
    .join(", ");
}

/** The noise floor with its verdict: `412 mm/s² (High)`. */
export function noiseText(result: AxesMapResult): string {
  return `${result.noise.toFixed(0)} mm/s² (${NOISE_WORDS[result.noise_verdict]})`;
}
