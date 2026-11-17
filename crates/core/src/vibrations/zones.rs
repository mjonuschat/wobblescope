//! Quiet zones of a curve: `identify_low_energy_zones` of Shake&Tune's `common_func.py` and
//! `_filter_and_split_ranges` of its `VibrationsComputation`.

/// A stretch of a curve below the quiet threshold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zone {
    /// Index of the first sample of the zone.
    pub start: usize,
    /// Index at which the zone ends. It is the first sample at or above the threshold, so it is
    /// outside the zone, except for a zone that runs to the end of the data, where it is the last
    /// sample and inside (D42).
    pub end: usize,
    /// Mean of the samples from `start` up to but not including `end`, as a percentage of the
    /// curve's maximum (D42).
    pub energy: f64,
}

/// The zones of `power` below `mean + (max - min) / 4 - threshold * std`, sorted from lowest to
/// highest `energy`. A zone whose mean is NaN is left out; that is the case of a one-sample zone
/// at the very end of the data. An empty curve has no zones.
pub fn low_energy_zones(power: &[f64], threshold: f64) -> Vec<Zone> {
    if power.is_empty() {
        return Vec::new();
    }
    let n = power.len() as f64;
    let max = power.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let min = power.iter().copied().fold(f64::INFINITY, f64::min);
    let mean = power.iter().sum::<f64>() / n;
    let std = (power.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n).sqrt();
    let limit = mean + (max - min) / 4.0 - threshold * std;

    let mut valleys: Vec<(usize, usize)> = Vec::new();
    let mut start = None;
    for (i, value) in power.iter().enumerate() {
        match start {
            None if *value < limit => start = Some(i),
            Some(s) if *value >= limit => {
                valleys.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        valleys.push((s, power.len() - 1));
    }

    let mut zones: Vec<Zone> = valleys
        .into_iter()
        .filter_map(|(start, end)| {
            let slice = &power[start..end];
            let zone_mean = slice.iter().sum::<f64>() / slice.len() as f64;
            (!zone_mean.is_nan()).then(|| Zone {
                start,
                end,
                energy: zone_mean / max * 100.0,
            })
        })
        .collect();
    zones.sort_by(|a, b| a.energy.total_cmp(&b.energy));
    zones
}

/// Cuts `deletion_range` samples either side of every peak out of the zones it falls in, then
/// merges zones that overlap, keeping the lower energy. The result is sorted by `start`, not by
/// energy, as Shake&Tune leaves it (D41).
///
/// `peak_speed_indices` pairs each peak speed with its index in `speeds`. A peak counts for a zone
/// when its speed lies between the speeds at the zone's `start` and `end`.
///
/// Shake&Tune fails with an `IndexError` when no zone is left (D40); this returns an empty list.
pub fn filter_and_split_zones(
    speeds: &[f64],
    zones: &[Zone],
    peak_speed_indices: &[(f64, usize)],
    deletion_range: usize,
) -> Vec<Zone> {
    let mut filtered: Vec<Zone> = Vec::new();
    for zone in zones {
        let (start_speed, end_speed) = (speeds[zone.start], speeds[zone.end]);
        let mut peaks: Vec<usize> = peak_speed_indices
            .iter()
            .filter(|(speed, _)| start_speed <= *speed && *speed <= end_speed)
            .map(|(_, index)| *index)
            .collect();
        if peaks.is_empty() {
            filtered.push(*zone);
            continue;
        }
        peaks.sort_unstable();
        let mut current_start = zone.start;
        for peak in peaks {
            let before_peak_end = current_start.max(peak.saturating_sub(deletion_range));
            if current_start < before_peak_end {
                filtered.push(Zone {
                    start: current_start,
                    end: before_peak_end,
                    energy: zone.energy,
                });
            }
            current_start = peak + deletion_range + 1;
        }
        if current_start < zone.end {
            filtered.push(Zone {
                start: current_start,
                end: zone.end,
                energy: zone.energy,
            });
        }
    }

    filtered.sort_by_key(|z| z.start);
    let mut merged: Vec<Zone> = Vec::new();
    for current in filtered {
        match merged.last_mut() {
            Some(last) if current.start <= last.end => {
                last.end = last.end.max(current.end);
                last.energy = last.energy.min(current.energy);
            }
            _ => merged.push(current),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_zone_list_after_peak_removal_is_empty_not_a_panic() {
        // D40: Shake&Tune raises IndexError here.
        let speeds: Vec<f64> = (0..10).map(f64::from).collect();
        let zone = Zone {
            start: 3,
            end: 5,
            energy: 1.0,
        };
        assert!(filter_and_split_zones(&speeds, &[zone], &[(4.0, 4)], 3).is_empty());
        assert!(filter_and_split_zones(&speeds, &[], &[], 3).is_empty());
    }

    #[test]
    fn flat_signal_has_one_zone_or_none_without_nan() {
        for value in [0.0, 0.1, 3.7] {
            let zones = low_energy_zones(&[value; 40], 1.1);
            assert!(zones.len() <= 1);
            assert!(zones.iter().all(|z| !z.energy.is_nan()));
        }
        assert!(low_energy_zones(&[], 0.7).is_empty());
    }

    #[test]
    fn zone_ending_at_the_data_end_is_inclusive_and_others_exclusive() {
        // D42
        let power = [5.0, 1.0, 1.0, 5.0, 5.0, 5.0, 5.0, 1.0, 1.0];
        let zones = low_energy_zones(&power, 0.1);
        let spans: Vec<(usize, usize)> = zones.iter().map(|z| (z.start, z.end)).collect();
        assert_eq!(spans, vec![(1, 3), (7, 8)]);
    }

    #[test]
    fn overlapping_zones_merge_keeping_the_lower_energy() {
        let speeds: Vec<f64> = (0..20).map(f64::from).collect();
        let zones = [
            Zone {
                start: 2,
                end: 8,
                energy: 4.0,
            },
            Zone {
                start: 6,
                end: 12,
                energy: 2.0,
            },
        ];
        let merged = filter_and_split_zones(&speeds, &zones, &[], 1);
        assert_eq!(
            merged,
            vec![Zone {
                start: 2,
                end: 12,
                energy: 2.0
            }]
        );
    }
}
