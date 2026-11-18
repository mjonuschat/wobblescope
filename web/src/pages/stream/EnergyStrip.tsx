import { useMemo } from "react";

import { EChart } from "@/components/charts/EChart";
import { WIDE_CHART } from "@/components/charts/size";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { formatHz } from "@/lib/format";
import type { Spectrum } from "@/lib/generated/Spectrum";
import { useChartTheme } from "@/ui-kit/chartTheme";
import { Panel } from "@/ui-kit/Panel";

import { bandOption } from "./bandOption";
import { BAND_SPAN_MS, useBandHistory } from "./useBandHistory";

/** The half widths of the band the strip measures, in Hz. */
export const BAND_HALF_WIDTHS = [1, 2, 5] as const;

/** The band either side of the frequency of interest to start with, in Hz. */
export const DEFAULT_BAND_HALF_WIDTH = 2;

interface Props {
  /** The frequency to measure around, in Hz. */
  centerHz: number;
  /** The width of the band either side of it, in Hz. */
  halfWidthHz: number;
  onHalfWidth: (hz: number) => void;
  /** The chips that are shown, with their colours and latest spectra. */
  chips: { chip: string; color: string; spectrum: Spectrum }[];
  /** The clock in milliseconds. Tests pass their own. */
  now?: () => number;
}

/**
 * How strongly each chip vibrates around the frequency of interest, over the last minute. It shows
 * what touching the toolhead, a belt or a joint does to a held frequency.
 */
export function EnergyStrip({ centerHz, halfWidthHz, onHalfWidth, chips, now = () => performance.now() }: Props) {
  const theme = useChartTheme();
  const spectra = useMemo(() => Object.fromEntries(chips.map(({ chip, spectrum }) => [chip, spectrum])), [chips]);
  const clock = now();
  const history = useBandHistory(spectra, centerHz, halfWidthHz, () => clock);
  const option = bandOption(
    chips.map(({ chip, color }) => ({
      chip,
      color,
      points: (history[chip] ?? []).map((p) => [(p.at - clock) / 1000, p.rms]),
    })),
    BAND_SPAN_MS / 1000,
    theme,
  );
  return (
    <Panel
      title={`Vibration around ${formatHz(centerHz)} Hz`}
      label="Vibration around the frequency of interest"
      action={
        <ToggleGroup
          variant="outline"
          size="sm"
          aria-label="Band width"
          value={[String(halfWidthHz)]}
          onValueChange={(next) => {
            if (next.length > 0) onHalfWidth(Number(next[0]));
          }}
        >
          {BAND_HALF_WIDTHS.map((hz) => (
            <ToggleGroupItem key={hz} value={String(hz)} aria-label={`Plus or minus ${hz} Hz`}>
              {`±${hz} Hz`}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      }
    >
      <EChart
        option={option}
        label={`RMS vibration around ${formatHz(centerHz)} Hz of the streamed chips`}
        height={200}
        minWidth={WIDE_CHART}
      />
    </Panel>
  );
}
