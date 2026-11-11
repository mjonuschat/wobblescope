import type { Peak } from "@/lib/generated/Peak";
import type { Spectrum } from "@/lib/generated/Spectrum";

export type SpectrumRequest =
  | { type: "configure"; chip: string; rateHz: number; windowSeconds: number; maxHz: number; minIntervalMs: number }
  | { type: "frame"; chip: string; t0: number; xyz: Float32Array }
  | { type: "drop"; chip: string };

export type SpectrumResponse =
  | { type: "spectrum"; chip: string; spectrum: Spectrum; peaks: Peak[] }
  | { type: "error"; chip: string; message: string };

/** One live spectrum open inside wasm. */
export interface LiveSpectrumHandle {
  push(t0: number, xyz: Float32Array): void;
  spectrum(): Spectrum | null;
  peaks(count: number, minSeparationHz: number): Peak[];
  free(): void;
}

/** The part of the wasm module the spectrum worker uses. */
export interface SpectrumModule {
  WasmLiveSpectrum: new (rateHz: number, windowSeconds: number, maxHz: number) => LiveSpectrumHandle;
}

/**
 * How many peaks are reported per chip, the least distance between two of them, and how high a
 * peak must reach, as a share of the strongest one, to be reported at all.
 */
export const PEAK_COUNT = 3;
export const PEAK_SEPARATION_HZ = 10;
export const PEAK_MIN_RATIO = 0.1;

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

interface Entry {
  handle: LiveSpectrumHandle;
  minIntervalMs: number;
  lastSentAt: number;
}

/**
 * Builds the spectrum worker's message handler around a module loader and a clock in
 * milliseconds. Each chip has its own window and answers with at most one spectrum per
 * `minIntervalMs`. A module that fails to load answers every request with that failure.
 */
export function createSpectrumHandler(loadModule: () => Promise<SpectrumModule>, now: () => number) {
  let loading: Promise<SpectrumModule> | null = null;
  const chips = new Map<string, Entry>();

  function release(chip: string) {
    chips.get(chip)?.handle.free();
    chips.delete(chip);
  }

  return async function handle(request: SpectrumRequest): Promise<SpectrumResponse | null> {
    try {
      loading ??= loadModule();
      const module = await loading;
      switch (request.type) {
        case "configure":
          release(request.chip);
          chips.set(request.chip, {
            handle: new module.WasmLiveSpectrum(request.rateHz, request.windowSeconds, request.maxHz),
            minIntervalMs: request.minIntervalMs,
            lastSentAt: Number.NEGATIVE_INFINITY,
          });
          return null;
        case "drop":
          release(request.chip);
          return null;
        case "frame": {
          const entry = chips.get(request.chip);
          if (!entry) return null;
          entry.handle.push(request.t0, request.xyz);
          const at = now();
          if (at - entry.lastSentAt < entry.minIntervalMs) return null;
          const spectrum = entry.handle.spectrum();
          if (!spectrum) return null;
          entry.lastSentAt = at;
          const found = entry.handle.peaks(PEAK_COUNT, PEAK_SEPARATION_HZ);
          const floor = (found[0]?.level ?? 0) * PEAK_MIN_RATIO;
          return {
            type: "spectrum",
            chip: request.chip,
            spectrum,
            peaks: found.filter((peak) => peak.level >= floor),
          };
        }
      }
    } catch (error) {
      return { type: "error", chip: request.chip, message: describe(error) };
    }
  };
}
