//! Small numpy-compatible helpers.

/// Longest sequence [`arange`] will build; longer requests yield an empty vector rather than
/// exhausting memory.
pub const MAX_ARANGE_LEN: usize = 1 << 24;

/// `numpy.arange` for floats: length `ceil((stop - start) / step)`, values `start + i * delta`
/// where `delta = (start + step) - start`, numpy's effective increment. Empty when the range is
/// empty, non-finite, or longer than [`MAX_ARANGE_LEN`].
pub fn arange(start: f64, stop: f64, step: f64) -> Vec<f64> {
    let len = ((stop - start) / step).ceil();
    if len.is_nan() || len <= 0.0 || len > MAX_ARANGE_LEN as f64 {
        return Vec::new();
    }
    let delta = (start + step) - start;
    (0..len as usize)
        .map(|i| start + i as f64 * delta)
        .collect()
}

/// Whether every timestamp is finite and each is greater than the one before. An empty or
/// one-element slice qualifies.
pub fn strictly_increasing(t: &[f64]) -> bool {
    t.iter().all(|v| v.is_finite()) && t.windows(2).all(|w| w[1] > w[0])
}

/// Sample rate in Hz from timestamps in seconds: one over the median of the positive intervals
/// between consecutive samples. Repeated or backward steps are ignored, so one out-of-order
/// sample does not change the answer; gaps barely move it. `None` when no positive interval
/// remains or the result is not finite.
pub fn median_rate(t: &[f64]) -> Option<f64> {
    let mut steps: Vec<f64> = t
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|d| d.is_finite() && *d > 0.0)
        .collect();
    if steps.is_empty() {
        return None;
    }
    let mid = steps.len() / 2;
    let (_, median, _) = steps.select_nth_unstable_by(mid, f64::total_cmp);
    let rate = 1.0 / *median;
    rate.is_finite().then_some(rate)
}

/// `numpy.trapz`: the trapezoid rule over the points `(x, y)`. Zero when fewer than two points
/// are given. Extra values in the longer slice are ignored.
pub fn trapz(y: &[f64], x: &[f64]) -> f64 {
    x.windows(2)
        .zip(y.windows(2))
        .map(|(x, y)| (x[1] - x[0]) * (y[0] + y[1]) / 2.0)
        .sum()
}

/// `numpy.convolve(a, v, mode='same')`: the centre of the full convolution, as long as the longer
/// input. numpy swaps the inputs when `v` is the longer one, and for an even kernel the centre
/// sits half a bin to the left of the one for an odd kernel. Empty when either input is empty.
pub fn convolve_same(a: &[f64], v: &[f64]) -> Vec<f64> {
    let (long, short) = if v.len() > a.len() { (v, a) } else { (a, v) };
    if short.is_empty() {
        return Vec::new();
    }
    let offset = short.len() - 1 - short.len() / 2;
    (0..long.len())
        .map(|i| {
            let c = i + offset;
            let lo = c.saturating_sub(short.len() - 1);
            let hi = c.min(long.len() - 1);
            (lo..=hi).map(|j| long[j] * short[c - j]).sum()
        })
        .collect()
}

/// A gap longer than this many median sample intervals degrades a recording.
pub const GAP_FACTOR: f64 = 3.0;

/// One note per gap in `t` longer than [`GAP_FACTOR`] median sample intervals. Only gaps overlapping `window` (start, end) count; `None` checks the
/// whole recording. Empty when no sample rate can be derived.
pub fn long_gaps(t: &[f64], window: Option<(f64, f64)>) -> Vec<String> {
    let Some(interval) = median_rate(t).map(|r| 1.0 / r) else {
        return Vec::new();
    };
    let limit = interval * GAP_FACTOR;
    t.windows(2)
        .filter(|w| {
            let in_window = window.is_none_or(|(start, end)| w[1] > start && w[0] < end);
            in_window && w[1] - w[0] > limit
        })
        .map(|w| {
            format!(
                "a gap of {:.1} ms at t={:.3} s, {:.1} times the sample interval",
                (w[1] - w[0]) * 1000.0,
                w[0],
                (w[1] - w[0]) / interval
            )
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum Place {
    /// The query is NaN, so the result is too.
    Nan,
    /// Below the first bin. The result is the first value.
    Low,
    /// At or above the last bin. The result is the last value.
    High,
    /// Between bins `j` and `j + 1`, `dv` above the first and `dx` apart.
    Between { j: usize, dv: f64, dx: f64 },
}

/// `numpy.interp` split in two. The bracketing of `x` in `xp` is found once, and
/// [`apply`](Self::apply) interpolates any `fp` on those bins, so one plan serves many `fp`.
#[derive(Debug, Clone)]
pub struct InterpPlan {
    places: Vec<Place>,
    xp_len: usize,
    /// `xp` is empty or not finite and strictly increasing, so every result is NaN.
    invalid: bool,
}

impl InterpPlan {
    pub fn new(x: &[f64], xp: &[f64]) -> Self {
        if xp.is_empty() || !strictly_increasing(xp) {
            return Self {
                places: vec![Place::Nan; x.len()],
                xp_len: xp.len(),
                invalid: true,
            };
        }
        let last = xp.len() - 1;
        let places = x
            .iter()
            .map(|&v| {
                if v.is_nan() {
                    Place::Nan
                } else if v < xp[0] {
                    Place::Low
                } else if v >= xp[last] {
                    Place::High
                } else {
                    let j = xp.partition_point(|&p| p <= v) - 1;
                    Place::Between {
                        j,
                        dv: v - xp[j],
                        dx: xp[j + 1] - xp[j],
                    }
                }
            })
            .collect();
        Self {
            places,
            xp_len: xp.len(),
            invalid: false,
        }
    }

    /// The values `fp` takes at the planned points. Every result is NaN when `xp` was invalid or
    /// `fp` differs in length from it.
    pub fn apply(&self, fp: &[f64]) -> Vec<f64> {
        if self.invalid || fp.len() != self.xp_len {
            return vec![f64::NAN; self.places.len()];
        }
        let last = fp.len() - 1;
        self.places
            .iter()
            .map(|place| match *place {
                Place::Nan => f64::NAN,
                Place::Low => fp[0],
                Place::High => fp[last],
                Place::Between { j, dv, dx } => {
                    let slope = (fp[j + 1] - fp[j]) / dx;
                    slope * dv + fp[j]
                }
            })
            .collect()
    }
}

/// `numpy.interp`: piecewise-linear interpolation of `(xp, fp)` at `x`, clamped outside `xp`.
/// If `xp` is empty, differs in length from `fp`, or is not finite and strictly increasing, every
/// result is NaN.
pub fn interp(x: &[f64], xp: &[f64], fp: &[f64]) -> Vec<f64> {
    InterpPlan::new(x, xp).apply(fp)
}

/// `numpy.linspace(start, stop, num)` with the endpoint included.
pub fn linspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 1 {
        return vec![start];
    }
    let step = (stop - start) / (num - 1) as f64;
    let mut out: Vec<f64> = (0..num).map(|i| i as f64 * step + start).collect();
    if let Some(last) = out.last_mut() {
        *last = stop;
    }
    out
}

/// `numpy.median` of sorted, non-empty values.
pub fn median_sorted(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// `numpy.percentile` of sorted, non-empty values, interpolating linearly between ranks.
pub fn percentile_sorted(sorted: &[f64], q: f64) -> f64 {
    let position = (sorted.len() - 1) as f64 * (q / 100.0);
    let below = position.floor();
    let t = position - below;
    let lo = sorted[below as usize];
    let hi = sorted[(below as usize + 1).min(sorted.len() - 1)];
    // numpy's `_lerp`, which counts from the nearer end.
    if t >= 0.5 {
        hi - (hi - lo) * (1.0 - t)
    } else {
        lo + (hi - lo) * t
    }
}

#[cfg(test)]
mod tests {
    use super::{convolve_same, trapz};

    // The expected values below come from numpy 2.x:
    // np.convolve([1,2,3], [1,1], mode='same') == [1 3 5]
    // np.convolve([1,2,3,4], [1,2,3], mode='same') == [4 10 16 17]
    // np.convolve([1,2], [1,2,3,4,5], mode='same') == [1 4 7 10 13]
    #[test]
    fn convolve_same_aligns_an_even_kernel_like_numpy() {
        assert_eq!(
            convolve_same(&[1.0, 2.0, 3.0], &[1.0, 1.0]),
            [1.0, 3.0, 5.0]
        );
    }

    #[test]
    fn convolve_same_centres_an_odd_kernel_like_numpy() {
        assert_eq!(
            convolve_same(&[1.0, 2.0, 3.0, 4.0], &[1.0, 2.0, 3.0]),
            [4.0, 10.0, 16.0, 17.0]
        );
    }

    #[test]
    fn convolve_same_with_a_kernel_longer_than_the_input_returns_the_longer_length() {
        assert_eq!(
            convolve_same(&[1.0, 2.0], &[1.0, 2.0, 3.0, 4.0, 5.0]),
            [1.0, 4.0, 7.0, 10.0, 13.0]
        );
    }

    #[test]
    fn trapz_integrates_uneven_spacing_like_numpy() {
        // numpy.trapz([1, 3, 2, 5], [0, 1, 3, 4]) == 2 + 5 + 3.5
        assert_eq!(trapz(&[1.0, 3.0, 2.0, 5.0], &[0.0, 1.0, 3.0, 4.0]), 10.5);
    }

    #[test]
    fn trapz_of_fewer_than_two_points_is_zero() {
        assert_eq!(trapz(&[], &[]), 0.0);
        assert_eq!(trapz(&[4.0], &[1.0]), 0.0);
    }
}
