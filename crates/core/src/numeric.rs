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
