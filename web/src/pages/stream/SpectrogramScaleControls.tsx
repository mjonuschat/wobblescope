import { UnitInput } from "@/components/UnitInput";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { type ColourScale, RAMP } from "@/stream/spectrogramScale";

interface Props {
  auto: boolean;
  onAuto: (auto: boolean) => void;
  /** The fixed limits as typed, in power. */
  min: string;
  max: string;
  onMin: (text: string) => void;
  onMax: (text: string) => void;
  scale: ColourScale;
}

const level = (logPower: number) => (10 ** logPower).toExponential(1);

const GRADIENT = `linear-gradient(to right, ${RAMP.map(([r, g, b]) => `rgb(${r} ${g} ${b})`).join(", ")})`;

export function SpectrogramScaleControls({ auto, onAuto, min, max, onMin, onMax, scale }: Props) {
  const shown = (typed: string, logPower: number) => (auto ? String(Number((10 ** logPower).toPrecision(2))) : typed);
  return (
    <div className="flex flex-wrap items-center gap-4">
      <div className="flex items-center gap-2">
        <Switch id="spectrogram-auto-scale" checked={auto} onCheckedChange={onAuto} />
        <Label htmlFor="spectrogram-auto-scale">Auto Scale</Label>
      </div>
      <UnitInput
        unit="PSD"
        min={0}
        aria-label="Spectrogram minimum"
        className="w-40"
        disabled={auto}
        value={shown(min, scale.low)}
        onChange={(event) => onMin(event.target.value)}
      />
      <UnitInput
        unit="PSD"
        min={0}
        aria-label="Spectrogram maximum"
        className="w-40"
        disabled={auto}
        value={shown(max, scale.low + scale.range)}
        onChange={(event) => onMax(event.target.value)}
      />
      <div
        role="img"
        aria-label={`Colour scale from ${level(scale.low)} to ${level(scale.low + scale.range)}`}
        className="ml-auto flex items-center gap-2 text-xs text-muted-foreground tabular-nums"
      >
        {level(scale.low)}
        <div className="h-3 w-40 rounded-sm border" style={{ background: GRADIENT }} />
        {level(scale.low + scale.range)}
      </div>
    </div>
  );
}
