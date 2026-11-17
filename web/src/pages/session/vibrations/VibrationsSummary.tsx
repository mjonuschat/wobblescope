import type { VibrationsResult } from "@/lib/generated/VibrationsResult";
import type { Zone } from "@/lib/generated/Zone";

const round1 = (value: number) => Number(value.toFixed(1));

/** The index of a zone's end can point one past the grid. Keep it on it. */
const at = (values: number[], index: number) => values[Math.max(0, Math.min(index, values.length - 1))];

const heading = "text-xs uppercase tracking-wide text-muted-foreground";

function ZoneList({
  title,
  noun,
  empty,
  zones,
  range,
}: {
  title: string;
  noun: string;
  empty: string;
  zones: Zone[];
  range: (zone: Zone) => string;
}) {
  return (
    <div className="space-y-1">
      <h3 className={heading}>{title}</h3>
      {zones.length === 0 ? (
        <p className="text-sm">{empty}</p>
      ) : (
        <ul className="space-y-0.5 text-sm">
          {zones.map((zone, index) => (
            <li key={`${zone.start}-${zone.end}`}>
              <span className="font-medium">{`${noun} ${index + 1}`}</span>
              {`: ${range(zone)}, ${zone.energy.toFixed(1)}% of max energy`}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function VibrationsSummary({ result }: { result: VibrationsResult }) {
  const { motor } = result;
  return (
    <div className="grid gap-6 md:grid-cols-3">
      <div className="space-y-1">
        <h3 className={heading}>Speeds to avoid</h3>
        {result.peak_speeds.length === 0 ? (
          <p className="text-sm">No vibration peaks were found.</p>
        ) : (
          <>
            <ul className="space-y-0.5 text-sm">
              {result.peak_speeds.map((speed, index) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: peaks are numbered by their order
                <li key={index}>{`${index + 1}: ${round1(speed)} mm/s`}</li>
              ))}
            </ul>
            <p className="text-sm text-muted-foreground">
              Avoid setting a speed near these values in your slicer print profile.
            </p>
          </>
        )}
      </div>
      <ZoneList
        title="Good speed ranges"
        noun="Zone"
        empty="No good speed ranges were found."
        zones={result.good_speeds}
        range={(zone) => `${round1(at(result.speeds, zone.start))} to ${round1(at(result.speeds, zone.end))} mm/s`}
      />
      <ZoneList
        title="Good angle ranges"
        noun="Angle range"
        empty="No good angle ranges were found."
        zones={result.good_angles}
        range={(zone) => `${round1(at(result.angles, zone.start))} to ${round1(at(result.angles, zone.end))} degrees`}
      />
      <dl className="grid grid-cols-[auto_1fr] content-start gap-x-4 gap-y-1 text-sm md:col-span-3">
        <dt className="text-muted-foreground">Symmetry</dt>
        <dd>{result.symmetry === null ? "not available" : `${result.symmetry.toFixed(1)}%`}</dd>
        {motor && (
          <>
            <dt className="text-muted-foreground">Motor resonance</dt>
            <dd>{`${motor.fr.toFixed(1)} Hz`}</dd>
            <dt className="text-muted-foreground">Motor damping ratio</dt>
            <dd>{motor.zeta === null ? "not available" : motor.zeta.toFixed(3)}</dd>
          </>
        )}
      </dl>
      {motor?.low_freq_warning && (
        <p role="status" className="text-sm text-amber-400 md:col-span-3">
          The motor resonance is low, so the damping ratio may be unreliable.
        </p>
      )}
    </div>
  );
}
