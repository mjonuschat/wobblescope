//! The `wasm-bindgen` surface. Values cross as JavaScript objects through `serde-wasm-bindgen`,
//! with a missing value written as `null` so it matches the generated `T | null` types.

use serde::Serialize;
use serde_wasm_bindgen::Serializer;
use wasm_bindgen::prelude::*;
use wobblescope_core::fit::{Axis, FitParams, Recommendation};

use crate::Session;

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
    pub fn analyze(&self, axis: JsValue, params: JsValue) -> Result<JsValue, JsError> {
        let axis: Axis = from_js("axis", axis)?;
        let params: FitParams = from_js("fit parameters", params)?;
        let analysis = self
            .inner
            .analyze(axis, &params)
            .map_err(|e| JsError::new(&e.to_string()))?;
        to_js(&analysis)
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
