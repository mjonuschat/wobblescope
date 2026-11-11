//! Streaming low-pass decimator that keeps the displayed range free of aliases.

use std::f64::consts::PI;

use crate::error::CoreError;
use crate::psd::kaiser;
use crate::samples::RawSamples;

/// Output rate must be at least this multiple of the displayed range.
const MIN_RATE_RATIO: f64 = 2.56;
/// Kaiser beta for about 60 dB of stopband attenuation.
const KAISER_BETA: f64 = 5.65;
/// Design attenuation in dB, with margin over the required 50 dB.
const ATTENUATION_DB: f64 = 60.0;

/// Low-pass filters and drops samples so a range `0..=max_hz` shows no aliases.
///
/// The passband edge is `max_hz` and the stopband edge is
/// `output_rate_hz - max_hz`, the lowest input frequency that folds into the
/// range. Phase and filter state carry across calls to [`Decimator::process`].
#[derive(Debug, Clone)]
pub struct Decimator {
    input_rate_hz: f64,
    factor: usize,
    /// Filter coefficients; empty when `factor == 1`.
    taps: Vec<f64>,
    /// Last `taps.len() - 1` inputs per axis (x, y, z), oldest first.
    history: [Vec<f64>; 3],
    /// Index of the next input sample, counted from the first one seen.
    next_index: usize,
}

impl Decimator {
    /// Designs a decimator for `input_rate_hz` input that must show `0..=max_hz`.
    ///
    /// Returns `CoreError::InvalidParameter` for a non-finite or non-positive
    /// rate or range, or a range above half the input rate.
    pub fn new(input_rate_hz: f64, max_hz: f64) -> Result<Self, CoreError> {
        for (what, v) in [("input_rate_hz", input_rate_hz), ("max_hz", max_hz)] {
            if !v.is_finite() || v <= 0.0 {
                return Err(CoreError::InvalidParameter(format!("{what} = {v:?}")));
            }
        }
        if max_hz > input_rate_hz / 2.0 {
            return Err(CoreError::InvalidParameter(format!(
                "max_hz = {max_hz:?} exceeds half of input_rate_hz = {input_rate_hz:?}"
            )));
        }

        let mut factor = 1;
        while input_rate_hz / (factor + 1) as f64 >= MIN_RATE_RATIO * max_hz {
            factor += 1;
        }
        if factor == 1 {
            return Ok(Self {
                input_rate_hz,
                factor,
                taps: Vec::new(),
                history: Default::default(),
                next_index: 0,
            });
        }

        let output_rate = input_rate_hz / factor as f64;
        let stop_edge = output_rate - max_hz;
        let cutoff = (max_hz + stop_edge) / 2.0 / input_rate_hz;
        let width = 2.0 * PI * (stop_edge - max_hz) / input_rate_hz;
        let mut count = ((ATTENUATION_DB - 8.0) / (2.285 * width)).ceil() as usize + 1;
        if count.is_multiple_of(2) {
            count += 1;
        }

        let mid = (count - 1) as f64 / 2.0;
        let window = kaiser(count, KAISER_BETA);
        let mut taps: Vec<f64> = (0..count)
            .map(|i| {
                let k = i as f64 - mid;
                let sinc = if k == 0.0 {
                    2.0 * cutoff
                } else {
                    (2.0 * PI * cutoff * k).sin() / (PI * k)
                };
                sinc * window[i]
            })
            .collect();
        let gain: f64 = taps.iter().sum();
        taps.iter_mut().for_each(|t| *t /= gain);

        Ok(Self {
            input_rate_hz,
            factor,
            history: std::array::from_fn(|_| vec![0.0; count - 1]),
            taps,
            next_index: 0,
        })
    }

    /// Keeps one input sample in every `factor()`.
    pub fn factor(&self) -> usize {
        self.factor
    }

    /// Sample rate of the output: `input_rate_hz / factor`.
    pub fn output_rate_hz(&self) -> f64 {
        self.input_rate_hz / self.factor as f64
    }

    /// Forgets the filter history and restarts the phase, as for a new stream.
    pub fn reset(&mut self) {
        self.history.iter_mut().for_each(|h| h.fill(0.0));
        self.next_index = 0;
    }

    /// Filters `s` and returns the kept samples, each with the timestamp of the
    /// input sample it stands for. Chunk sizes do not change the result.
    pub fn process(&mut self, s: &RawSamples) -> RawSamples {
        if self.factor == 1 {
            return s.clone();
        }
        let first = self.next_index;
        let kept: Vec<usize> = (0..s.len())
            .filter(|i| (first + i).is_multiple_of(self.factor))
            .collect();
        let t = kept.iter().map(|&i| s.t[i]).collect();
        let [x, y, z] = [(&s.x, 0), (&s.y, 1), (&s.z, 2)]
            .map(|(input, axis)| self.filter_axis(input, axis, &kept));
        self.next_index += s.len();
        RawSamples { t, x, y, z }
    }

    /// Filters only the input positions in `kept` and updates the axis history.
    fn filter_axis(&mut self, input: &[f64], axis: usize, kept: &[usize]) -> Vec<f64> {
        let history = &mut self.history[axis];
        let lead = history.len();
        let mut buf = Vec::with_capacity(lead + input.len());
        buf.extend_from_slice(history);
        buf.extend_from_slice(input);

        // Output for input i is the dot of taps with buf[i..=i + lead], newest last.
        let out = kept
            .iter()
            .map(|&i| {
                self.taps
                    .iter()
                    .zip(buf[i..=i + lead].iter().rev())
                    .map(|(h, v)| h * v)
                    .sum()
            })
            .collect();
        history.copy_from_slice(&buf[buf.len() - lead..]);
        out
    }
}
