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

/// `numpy.interp`: piecewise-linear interpolation of `(xp, fp)` at `x`, clamped outside `xp`.
/// If `xp` is empty, differs in length from `fp`, or is not finite and strictly increasing, every
/// result is NaN.
pub fn interp(x: &[f64], xp: &[f64], fp: &[f64]) -> Vec<f64> {
    if xp.is_empty()
        || xp.len() != fp.len()
        || xp.iter().any(|v| !v.is_finite())
        || xp.windows(2).any(|w| w[1] <= w[0])
    {
        return vec![f64::NAN; x.len()];
    }
    let last = xp.len() - 1;
    x.iter()
        .map(|&v| {
            if v.is_nan() {
                v
            } else if v < xp[0] {
                fp[0]
            } else if v >= xp[last] {
                fp[last]
            } else {
                let j = xp.partition_point(|&p| p <= v) - 1;
                let slope = (fp[j + 1] - fp[j]) / (xp[j + 1] - xp[j]);
                slope * (v - xp[j]) + fp[j]
            }
        })
        .collect()
}
