//! The `wasm-bindgen` surface. Values cross as JavaScript objects through `serde-wasm-bindgen`,
//! with a missing value written as `null` so it matches the generated `T | null` types.

use serde::Serialize;
use serde_wasm_bindgen::Serializer;
use wasm_bindgen::prelude::*;
use wobblescope_core::fit::{Axis, FitParams, Recommendation};

use crate::{Live, Session};

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    value
        .serialize(&Serializer::new().serialize_missing_as_null(true))
        .map_err(|e| JsError::new(&e.to_string()))
}

fn from_js<T: serde::de::DeserializeOwned>(what: &str, value: JsValue) -> Result<T, JsError> {
    serde_wasm_bindgen::from_value(value).map_err(|e| JsError::new(&format!("{what}: {e}")))
}

/// A session opened from its metadata, to which compressed sample files are added by name.
#[wasm_bindgen]
pub struct WasmSession {
    inner: Session,
}

#[wasm_bindgen]
impl WasmSession {
    /// Opens a session from `SessionDetail.meta`.
    #[wasm_bindgen(constructor)]
    pub fn new(meta: JsValue) -> Result<WasmSession, JsError> {
        let meta = from_js("session metadata", meta)?;
        let inner = Session::new(meta).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { inner })
    }

    /// Adds one stored sample file under its `sample_file` name.
    pub fn add_file(&mut self, name: String, bytes: Vec<u8>) {
        self.inner.add_file(&name, bytes);
    }

    /// Analyses one axis (`"X"` or `"Y"`). Fit parameters left out take Klipper's defaults.
    /// `overrides` names chips to include although their health or coverage would exclude them.
    pub fn analyze(
        &self,
        axis: JsValue,
        params: JsValue,
        overrides: Option<Vec<String>>,
    ) -> Result<JsValue, JsError> {
        let axis: Axis = from_js("axis", axis)?;
        let params: FitParams = from_js("fit parameters", params)?;
        let analysis = self
            .inner
            .analyze(axis, &params, &overrides.unwrap_or_default())
            .map_err(|e| JsError::new(&e.to_string()))?;
        to_js(&analysis)
    }

    /// The time-frequency map of one chip's recording at one probe point of `axis`, up to
    /// `max_freq` Hz.
    pub fn spectrogram(
        &self,
        axis: JsValue,
        chip: String,
        point: usize,
        max_freq: f64,
    ) -> Result<JsValue, JsError> {
        let axis: Axis = from_js("axis", axis)?;
        let map = self
            .inner
            .spectrogram(axis, &chip, point, max_freq)
            .map_err(|e| JsError::new(&e.to_string()))?;
        to_js(&map)
    }
}

/// A live spectrum over the latest seconds of a streamed accelerometer.
#[wasm_bindgen]
pub struct WasmLiveSpectrum {
    inner: Live,
}

#[wasm_bindgen]
impl WasmLiveSpectrum {
    /// Opens a window of `window_seconds` at `rate_hz`, reporting bins up to `max_hz`.
    /// Throws on bad parameters.
    #[wasm_bindgen(constructor)]
    pub fn new(
        rate_hz: f64,
        window_seconds: f64,
        max_hz: f64,
    ) -> Result<WasmLiveSpectrum, JsError> {
        let inner =
            Live::new(rate_hz, window_seconds, max_hz).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { inner })
    }

    /// Appends interleaved x, y, z samples. Sample `i` is at `t0 + i / rate_hz`.
    pub fn push(&mut self, t0: f64, xyz: &[f32]) -> Result<(), JsError> {
        self.inner
            .push(t0, xyz)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// The current `Spectrum`, or `null` until the window holds one full segment.
    pub fn spectrum(&self) -> Result<JsValue, JsError> {
        to_js(&self.inner.spectrum())
    }

    /// The strongest `Peak[]` of the current summed spectrum, empty when there is none.
    pub fn peaks(&self, count: usize, min_separation_hz: f64) -> Result<JsValue, JsError> {
        to_js(&self.inner.peaks(count, min_separation_hz))
    }
}

/// The `[input_shaper]` snippet for a recommendation and axis.
#[wasm_bindgen]
pub fn config_snippet(recommendation: JsValue, axis: JsValue) -> Result<String, JsError> {
    let recommendation: Recommendation = from_js("recommendation", recommendation)?;
    let axis: Axis = from_js("axis", axis)?;
    Ok(crate::config_snippet(&recommendation, axis))
}

/// Version of `core` that runs in the browser.
#[wasm_bindgen]
pub fn core_version() -> String {
    crate::core_version().to_owned()
}
