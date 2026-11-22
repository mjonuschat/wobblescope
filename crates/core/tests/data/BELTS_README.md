# Belt PSD fixtures

Two belt-comparison inputs for the belt level-imbalance work. Each `.json.zst` is a JSON array
with one entry per accelerometer chip: `name`, `extract_max_freq`, `sweep` (`[FREQ_START,
FREQ_END]` or `null`), and the four arrays `freqs_a`, `psd_a`, `freqs_b`, `psd_b`. A is the belt
recorded at `AXIS=1,-1`, B the one at `AXIS=1,1`.

The PSDs are what production builds: `analyze_belts` per chip (each recording cut to its time
window, summed X+Y+Z PSD, not normalised). Welch settings are those of
`CalibrationData::from_samples`: a Kaiser window (beta 6), 0.5 s long rounded up to a power-of-two
sample count, 50% overlap. The grid is the recording's own bins (spacing about 1.5 Hz), kept up
to the extraction limit `max_freq` 200 Hz. Both files are extracted at 200 Hz because
`analyze_belts` drops bins above its limit; a comparison at 150 Hz passes `max_freq` 150 to
`compare_psds_in_band` on these same bins.

- `belts_unequal_power.json.zst`: a saved belts result from a CoreXY printer with two
  accelerometers (chips `adxl345 carto` and `adxl345 nh36`), one probe point, about 375 s per
  segment, a long continuous run and no recorded sweep (`sweep` is `null`).
- `belts_balanced.json.zst`: a belts session from a CoreXY printer with an `lis2dw ebb36` chip,
  swept 5 to 135 Hz (`sweep` is `[5, 135]`), one probe point.

There is no regeneration script: the source recordings are not in the repository.

## Baselines before the level-imbalance change

`compare_psds_in_band` as it is before the level-imbalance change, for Task 2 to assert against.

| Fixture | Chip | Comparison `max_freq` | Sweep | Similarity | Pairs | Health |
| --- | --- | --- | --- | --- | --- | --- |
| unequal power | adxl345 carto | 150 | none | 99.6266 | 1 | Excellent mechanical health |
| unequal power | adxl345 carto | 200 | none | 99.5099 | 1 | Excellent mechanical health |
| unequal power | adxl345 nh36 | 150 | none | 99.1862 | 2 | Excellent mechanical health |
| unequal power | adxl345 nh36 | 200 | none | 99.0859 | 2 | Excellent mechanical health |
| balanced | lis2dw ebb36 | 150 | 5 to 135 | 96.2250 | 4 | Potential signs of a mechanical issue |
| balanced | lis2dw ebb36 | 200 | 5 to 135 | 96.2215 | 3 | Good mechanical health |
