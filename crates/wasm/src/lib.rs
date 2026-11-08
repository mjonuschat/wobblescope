//! The browser's route into `wobblescope-core` and `wobblescope-session`.
//!
//! The logic is plain Rust ([`Session`], [`config_snippet`]) so it builds and tests natively.
//! The `#[wasm_bindgen]` wrappers in [`js`] only convert values and errors. Compressed sample
//! files are copied into wasm memory once and decoded per analysis, so samples never cross the
//! boundary as JavaScript arrays.

use std::collections::HashMap;

use thiserror::Error;
use wobblescope_core::CORE_VERSION;
use wobblescope_core::error::CoreError;
use wobblescope_core::fit::{Axis, FitParams, Recommendation};
use wobblescope_core::multichip::{Analysis, analyze};
use wobblescope_session::SessionError;
use wobblescope_session::bridge::{apply_overrides, dataset_inputs};
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

    /// Analyses every recording of `axis` with `core`. Files are decoded one at a time for the
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
        for file in self
            .meta
            .segments
            .iter()
            .filter(|s| s.axis == axis)
            .flat_map(|s| &s.files)
        {
            if !self.files.contains_key(&file.sample_file) {
                return Err(AnalyzeError::MissingFile(file.sample_file.clone()));
            }
        }
        let mut inputs = dataset_inputs(&self.meta, axis, |file| {
            decode_samples(&self.files[&file.sample_file])
        })?;
        apply_overrides(&mut inputs, overrides);
        Ok(analyze(&inputs, params)?)
    }
}

/// The Klipper `[input_shaper]` lines for `axis`, as `core` renders them.
pub fn config_snippet(recommendation: &Recommendation, axis: Axis) -> String {
    recommendation.config_snippet(axis)
}

/// Version of `core` that runs here.
pub fn core_version() -> &'static str {
    CORE_VERSION
}
