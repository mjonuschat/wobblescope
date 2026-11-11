//! The travel to a hold's start point, cut into short segments.
//!
//! One long move cannot be stopped, so the travel is sent as segments of a fixed motion time that
//! the runner paces like pieces. Stop and a lost connection then end it between segments.

/// The absolute targets of the travel from `from` to `to` at `speed` mm/s, each at most
/// `speed * max_segment_seconds` mm from the last and the last exactly `to`. Empty when there is
/// no distance to cover.
#[must_use]
pub fn segments(
    from: [f64; 3],
    to: [f64; 3],
    speed: f64,
    max_segment_seconds: f64,
) -> Vec<[f64; 3]> {
    let delta = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let distance = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    if distance < 1e-9 {
        return Vec::new();
    }
    let step = speed * max_segment_seconds;
    let count = (distance / step - 1e-9).ceil().max(1.0) as usize;
    let mut out: Vec<[f64; 3]> = (1..=count)
        .map(|i| {
            let along = i as f64 / count as f64;
            [
                from[0] + delta[0] * along,
                from[1] + delta[1] * along,
                from[2] + delta[2] * along,
            ]
        })
        .collect();
    if let Some(last) = out.last_mut() {
        *last = to;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    #[test]
    fn a_long_move_is_cut_into_short_segments() {
        let out = segments([0.0, 0.0, 20.0], [100.0, 0.0, 20.0], 50.0, 0.5);
        assert_eq!(out.len(), 4);
        for (i, target) in out.iter().enumerate() {
            assert!(
                (target[0] - 25.0 * (i as f64 + 1.0)).abs() < 1e-9,
                "{target:?}"
            );
        }
        assert_eq!(*out.last().unwrap(), [100.0, 0.0, 20.0]);
    }

    #[test]
    fn a_short_move_is_one_segment() {
        let out = segments([0.0, 0.0, 20.0], [10.0, 0.0, 20.0], 50.0, 0.5);
        assert_eq!(out, vec![[10.0, 0.0, 20.0]]);
    }

    #[test]
    fn no_move_gives_no_segments() {
        assert!(segments([5.0, 5.0, 5.0], [5.0, 5.0, 5.0], 50.0, 0.5).is_empty());
    }

    #[test]
    fn a_diagonal_keeps_its_direction() {
        let from = [10.0, 20.0, 5.0];
        let to = [90.0, 100.0, 45.0];
        let out = segments(from, to, 40.0, 0.5);
        let mut last = from;
        let total = dist(from, to);
        for target in &out {
            assert!(dist(last, *target) <= 20.0 + 1e-9);
            let along = dist(from, *target) / total;
            for i in 0..3 {
                let want = from[i] + (to[i] - from[i]) * along;
                assert!((target[i] - want).abs() < 1e-9, "{target:?} off the line");
            }
            last = *target;
        }
        assert_eq!(*out.last().unwrap(), to);
    }
}
