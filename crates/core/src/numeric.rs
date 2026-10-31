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
