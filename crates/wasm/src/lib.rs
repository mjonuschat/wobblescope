//! The browser's route into `wobblescope-core` and `wobblescope-session`.
//!
//! The logic is plain Rust ([`Session`], [`config_snippet`]) so it builds and tests natively.
//! The `#[wasm_bindgen]` wrappers in [`js`] only convert values and errors. Compressed sample
//! files are copied into wasm memory once and decoded per analysis, so samples never cross the
//! boundary as JavaScript arrays.

use std::collections::HashMap;

use thiserror::Error;
use wobblescope_core::CORE_VERSION;
use wobblescope_core::axes_map::analyze_axes_map as core_analyze_axes_map;
use wobblescope_core::belts_run::{BeltsAnalysis, analyze_belts};
use wobblescope_core::direction::Direction;
use wobblescope_core::eligibility::DatasetInput;
use wobblescope_core::error::CoreError;
use wobblescope_core::fit::{
    Axis, FitParams, Recommendation, sweep_max_freq, sweep_max_shaper_freq,
};
use wobblescope_core::multichip::{Analysis, analyze};
use wobblescope_core::spectrogram::{Spectrogram, spectrogram_of};
use wobblescope_core::spectrum::{LiveSpectrum, Peak, Spectrum, top_peaks};
use wobblescope_session::SessionError;
use wobblescope_session::bridge::{
    AxesMapChip, ChipCapture, VibrationsAnalysis, analyze_vibrations, apply_overrides,
    axes_map_inputs, axes_map_params, belt_inputs, dataset_inputs,
};
use wobblescope_session::codec::decode_samples;
use wobblescope_session::meta::SessionMeta;

pub mod js;

/// Why an analysis could not run. The message is what the browser shows.
#[derive(Debug, Error)]
pub enum AnalyzeError {
    /// A sample file the axis needs was never added.
    #[error("sample file {0} is not loaded")]
    MissingFile(String),
    /// The metadata was refused by the session crate.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// `core` refused the data or the fit parameters.
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("no recording of chip {chip} at point {point} on this axis")]
    NoSuchDataset { chip: String, point: usize },
}

/// One session's metadata and its compressed sample files, held in wasm memory.
#[derive(Debug)]
pub struct Session {
    meta: SessionMeta,
    files: HashMap<String, Vec<u8>>,
}

impl Session {
    /// Starts a session from its metadata.
    ///
    /// # Errors
    /// A rule violation in `meta`.
    pub fn new(meta: SessionMeta) -> Result<Self, SessionError> {
        meta.validate()?;
        Ok(Self {
            meta,
            files: HashMap::new(),
        })
    }

    /// Keeps one compressed sample file under its `sample_file` name, replacing an earlier one.
    pub fn add_file(&mut self, name: &str, bytes: Vec<u8>) {
        self.files.insert(name.to_owned(), bytes);
    }

    /// Analyses every recording of `axis` with `core`. When `params` sets no `max_freq` and the
    /// session recorded its sweep, the fit reads up to [`sweep_max_freq`] of the sweep end, and likewise the shaper search up to
    /// [`sweep_max_shaper_freq`] when it sets no `max_shaper_freq`. Files are decoded one at a time for the
    /// call only. The chips named in `overrides` are included even when their health or
    /// coverage would exclude them; the result's eligibility says so. A damaged file, or one
    /// holding a different number of samples than the metadata records, is reported in the
    /// result as dropped or failed rather than stopping the analysis.
    ///
    /// # Errors
    /// A file of the axis that was never added, or a [`CoreError`] from the analysis.
    pub fn analyze(
        &self,
        axis: Axis,
        params: &FitParams,
        overrides: &[String],
    ) -> Result<Analysis, AnalyzeError> {
        let mut inputs = self.inputs(axis)?;
        apply_overrides(&mut inputs, overrides);
        let mut params = params.clone();
        if let Some(sweep) = &self.meta.settings {
            params.max_freq = params.max_freq.or(Some(sweep_max_freq(sweep.freq_end)));
            params.max_shaper_freq = params
                .max_shaper_freq
                .or(Some(sweep_max_shaper_freq(sweep.freq_end)));
        }
        Ok(analyze(&inputs, &params)?)
    }

    /// The time-frequency map of the recording of `chip` at probe point `point` on `axis`, up to
    /// `max_freq`, cut to the segment's time window as `analyze` cuts it. It is made for any
    /// recording that loaded, whatever its health, since a bad capture is worth a look.
    ///
    /// # Errors
    /// A file of the axis that was never added, [`AnalyzeError::NoSuchDataset`] for a chip or
    /// point the axis has no recording of, or a [`CoreError`] from the map.
    pub fn spectrogram(
        &self,
        axis: Axis,
        chip: &str,
        point: usize,
        max_freq: f64,
    ) -> Result<Spectrogram, AnalyzeError> {
        let inputs = self.inputs(axis)?;
        let input = inputs
            .iter()
            .find(|i| i.chip == chip && i.point == point)
            .ok_or_else(|| AnalyzeError::NoSuchDataset {
                chip: chip.to_owned(),
                point,
            })?;
        Ok(spectrogram_of(input, max_freq)?)
    }

    /// Compares the belt A and belt B recordings of `chip` up to `max_freq`. Only that chip's
    /// files are decoded, for the call only. With `include_excluded` a recording that health or
    /// coverage would exclude is used anyway. A belt the chip has no recording of is reported
    /// as absent, not as an error.
    ///
    /// # Errors
    /// A file of the chip's belt recordings that was never added.
    pub fn belts(
        &self,
        chip: &str,
        max_freq: f64,
        include_excluded: bool,
    ) -> Result<BeltsAnalysis, AnalyzeError> {
        for file in self
            .meta
            .segments
            .iter()
            .filter(|s| s.axis == Direction::BELT_A || s.axis == Direction::BELT_B)
            .flat_map(|s| &s.files)
            .filter(|f| self.meta.chips[f.chip as usize].name == chip)
        {
            if !self.files.contains_key(&file.sample_file) {
                return Err(AnalyzeError::MissingFile(file.sample_file.clone()));
            }
        }
        let (point, mut a, mut b) = belt_inputs(&self.meta, chip, |file| {
            decode_samples(&self.files[&file.sample_file])
        })?;
        for input in a.iter_mut().chain(b.iter_mut()) {
            input.include_override = include_excluded;
        }
        Ok(analyze_belts(
            chip,
            point,
            a.as_ref(),
            b.as_ref(),
            max_freq,
            self.meta
                .settings
                .as_ref()
                .map(|s| (s.freq_start, s.freq_end)),
        ))
    }

    /// The vibrations profile of every chip of a vibrations session and their worst case, up to
    /// `max_freq` Hz. Files are decoded one at a time for the call only. With `include_excluded`
    /// a chip that health or coverage would leave out of the worst case takes part anyway. A
    /// file that was never added makes that chip's profile a failure, not an error of the call.
    ///
    /// # Errors
    /// [`AnalyzeError::Session`] when the session is not a complete vibrations session.
    pub fn vibrations(
        &self,
        max_freq: f64,
        include_excluded: bool,
    ) -> Result<VibrationsAnalysis, AnalyzeError> {
        Ok(analyze_vibrations(
            &self.meta,
            max_freq,
            include_excluded,
            |file| {
                let bytes =
                    self.files
                        .get(&file.sample_file)
                        .ok_or_else(|| SessionError::Invalid {
                            field: file.sample_file.clone(),
                            reason: "sample file is not loaded".to_owned(),
                        })?;
                decode_samples(bytes)
            },
        )?)
    }

    /// The datasets of `axis`, with their samples decoded for the call only.
    fn inputs(&self, axis: Axis) -> Result<Vec<DatasetInput>, AnalyzeError> {
        for file in self
            .meta
            .segments
            .iter()
            .filter(|s| s.axis == axis.into())
            .flat_map(|s| &s.files)
        {
            if !self.files.contains_key(&file.sample_file) {
                return Err(AnalyzeError::MissingFile(file.sample_file.clone()));
            }
        }
        Ok(dataset_inputs(&self.meta, axis, |file| {
            decode_samples(&self.files[&file.sample_file])
        })?)
    }

    /// Analyses every chip in an axes-map calibration session. A chip whose files are not all
    /// loaded is reported as [`AxesMapChip::Incomplete`], not an error of the call. A chip whose
    /// analysis fails is reported as [`AxesMapChip::Failed`]; the other chips are still returned.
    ///
    /// # Errors
    /// [`AnalyzeError::Session`] when `meta` is not a valid axes-map session.
    pub fn analyze_axes_map(&self) -> Result<Vec<AxesMapChip>, AnalyzeError> {
        let params = axes_map_params(&self.meta)?;
        // `axes_map_inputs` calls `axes_map_params` again internally. The call above is kept to
        // surface a session error before allocating the closure and iterating files.
        let captures = axes_map_inputs(&self.meta, |file| {
            let bytes = self
                .files
                .get(&file.sample_file)
                .ok_or_else(|| SessionError::Invalid {
                    field: file.sample_file.clone(),
                    reason: "sample file is not loaded".to_owned(),
                })?;
            decode_samples(bytes)
        })?;
        let mut out = Vec::with_capacity(captures.len());
        for capture in captures {
            match capture {
                ChipCapture::Incomplete { chip, missing } => {
                    out.push(AxesMapChip::Incomplete { chip, missing });
                }
                ChipCapture::Ready {
                    input,
                    health,
                    health_notes,
                } => {
                    let chip = input.chip.clone();
                    match core_analyze_axes_map(&input, &params) {
                        Ok(result) => out.push(AxesMapChip::Analysed {
                            chip,
                            health,
                            health_notes,
                            result,
                        }),
                        Err(e) => out.push(AxesMapChip::Failed {
                            chip,
                            message: e.to_string(),
                        }),
                    }
                }
            }
        }
        Ok(out)
    }
}

/// Most samples per axis a live window may hold, so a mistaken or hostile caller cannot make
/// the browser allocate gigabytes.
pub const MAX_LIVE_WINDOW_SAMPLES: f64 = 4_000_000.0;

/// A live spectrum over the latest seconds of a streamed accelerometer.
#[derive(Debug, Clone)]
pub struct Live {
    inner: LiveSpectrum,
}

impl Live {
    /// Opens a live window of `window_seconds` at `rate_hz`, reporting bins up to `max_hz`.
    ///
    /// # Errors
    /// `InvalidParameter` for a bad argument or a window above [`MAX_LIVE_WINDOW_SAMPLES`].
    pub fn new(rate_hz: f64, window_seconds: f64, max_hz: f64) -> Result<Self, CoreError> {
        if rate_hz * window_seconds > MAX_LIVE_WINDOW_SAMPLES {
            return Err(CoreError::InvalidParameter(format!(
                "a window of {window_seconds} s at {rate_hz} Hz exceeds \
                 {MAX_LIVE_WINDOW_SAMPLES} samples"
            )));
        }
        Ok(Self {
            inner: LiveSpectrum::new(rate_hz, window_seconds, max_hz)?,
        })
    }

    /// Appends interleaved x, y, z samples starting at time `t0`; see
    /// [`LiveSpectrum::push_f32`].
    ///
    /// # Errors
    /// As [`LiveSpectrum::push_f32`].
    pub fn push(&mut self, t0: f64, xyz: &[f32]) -> Result<(), CoreError> {
        self.inner.push_f32(t0, xyz)
    }

    /// The current spectrum, or `None` until the window holds one full segment.
    pub fn spectrum(&self) -> Option<Spectrum> {
        self.inner.spectrum()
    }

    /// The strongest peaks of the current summed spectrum, empty when there is none.
    pub fn peaks(&self, count: usize, min_separation_hz: f64) -> Vec<Peak> {
        self.spectrum().map_or_else(Vec::new, |s| {
            top_peaks(&s.freq_bins, &s.psd_sum, count, min_separation_hz)
        })
    }
}

/// The Klipper `[input_shaper]` lines for `axis`, as `core` renders them. `source` names the chip
/// (or `all chips`) in the header comment.
pub fn config_snippet(recommendation: &Recommendation, axis: Axis, source: &str) -> String {
    recommendation.config_snippet(axis, source)
}

/// Version of `core` that runs here.
pub fn core_version() -> &'static str {
    CORE_VERSION
}
