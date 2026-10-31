//! Shared helpers for the golden-file suites. Data is embedded with `include_str!` so the same
//! tests run natively and on wasm32.
#![allow(dead_code)]

use std::collections::BTreeMap;

use serde::Deserialize;
use wobblescope_core::RawSamples;

/// Level-1 relative tolerance: PSD and per-frequency vibration/smoothing/score. The worst
/// deviation seen across all goldens is about 1.5e-11 (the DC bin, a cancellation artifact),
/// natively; wasm32 passes at this value too.
pub const REL_TOL: f64 = 1e-9;
/// Level-1 absolute floor for scalars that may be numerically zero.
pub const ABS_TOL: f64 = 1e-12;
/// Level-1 absolute floor for arrays, as a fraction of the array's peak magnitude, so near-zero
/// bins (DC, spectral tails) are judged against the scale of the spectrum they belong to.
pub const ABS_PEAK_TOL: f64 = 1e-12;
/// Level-3: scores within this relative margin count as a near tie.
pub const NEAR_TIE_MARGIN: f64 = 0.01;

/// Runs the wrapped test on both native and wasm32.
macro_rules! parity_test {
    ($(#[$m:meta])* fn $name:ident() $body:block) => {
        $(#[$m])*
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        fn $name() $body
    };
}

/// Logs a note about a tolerated difference.
pub fn log(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{msg}");
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::console_log!("{}", msg);
}

fn close_within(label: &str, got: f64, want: f64, abs_floor: f64) {
    let tol = abs_floor + REL_TOL * want.abs();
    assert!(
        (got - want).abs() <= tol,
        "{label}: got {got:e}, want {want:e}, diff {:e} > tol {tol:e}",
        (got - want).abs()
    );
}

pub fn close(label: &str, got: f64, want: f64) {
    close_within(label, got, want, ABS_TOL);
}

pub fn assert_close(label: &str, got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len(), "{label}: length");
    let floor = ABS_PEAK_TOL * want.iter().fold(0.0_f64, |m, w| m.max(w.abs()));
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        close_within(&format!("{label}[{i}]"), *g, *w, floor);
    }
}

pub fn csv(name: &str) -> &'static str {
    match name {
        "chip_a" => include_str!("../data/synthetic/chip_a.csv"),
        "chip_b" => include_str!("../data/synthetic/chip_b.csv"),
        _ => panic!("unknown csv {name}"),
    }
}

pub fn load_csv(name: &str) -> RawSamples {
    let mut s = RawSamples::default();
    for line in csv(name)
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let v: Vec<f64> = line.split(',').map(|f| f.parse().unwrap()).collect();
        s.t.push(v[0]);
        s.x.push(v[1]);
        s.y.push(v[2]);
        s.z.push(v[3]);
    }
    s
}

#[derive(Deserialize)]
pub struct Psd {
    pub freq_bins: Vec<f64>,
    pub psd_x: Vec<f64>,
    pub psd_y: Vec<f64>,
    pub psd_z: Vec<f64>,
    pub psd_sum: Vec<f64>,
}

#[derive(Deserialize)]
pub struct Dataset {
    pub name: String,
    pub raw: Psd,
    pub normalized: Psd,
}

#[derive(Deserialize)]
pub struct Cand {
    pub freq: f64,
    pub vibrs: f64,
    pub smoothing: f64,
    pub score: f64,
    pub max_accel: f64,
}

#[derive(Deserialize)]
pub struct Final {
    pub freq: f64,
    pub vibrs: f64,
    pub smoothing: f64,
    pub score: f64,
    pub max_accel: f64,
    pub freq_bins: Vec<f64>,
    pub vals: Vec<f64>,
}

#[derive(Deserialize)]
pub struct FitEntry {
    pub selected: Cand,
    pub candidates: Vec<Cand>,
    #[serde(rename = "final")]
    pub fin: Final,
}

#[derive(Deserialize)]
pub struct Best {
    pub name: String,
    pub freq: f64,
}

#[derive(Deserialize)]
pub struct Ranking {
    pub name: String,
    pub freq: f64,
    pub score: f64,
    pub vibrs: f64,
    pub smoothing: f64,
}

#[derive(Deserialize)]
pub struct Params {
    pub scv: f64,
    pub damping_ratio: Option<f64>,
    pub max_smoothing: Option<f64>,
    pub max_vibrations: Option<f64>,
    pub test_damping_ratios: Option<Vec<f64>>,
    pub max_freq: Option<f64>,
}

#[derive(Deserialize)]
pub struct Case {
    pub inputs: Vec<String>,
    pub params: Params,
    pub datasets: Vec<Dataset>,
    pub fit: BTreeMap<String, FitEntry>,
    pub best: Best,
    pub ranking: Vec<Ranking>,
    pub best_index: usize,
}

#[derive(Deserialize)]
pub struct ShaperRow {
    pub name: String,
    pub freq: f64,
    pub damping_ratio: f64,
    pub a: Vec<f64>,
    pub t: Vec<f64>,
}

pub fn shapers() -> Vec<ShaperRow> {
    serde_json::from_str(include_str!("../golden/shapers.json")).unwrap()
}

pub fn case(name: &str) -> Case {
    macro_rules! golden {
        ($($n:literal),* $(,)?) => {
            match name {
                $($n => include_bytes!(concat!("../golden/", $n, ".json.zst")) as &[u8],)*
                _ => panic!("unknown golden case {name}"),
            }
        };
    }
    let bytes = golden!(
        "chip_a",
        "chip_b",
        "chip_a+chip_b",
        "chip_a.max_smoothing",
        "chip_a.scv10_damp05",
        "chip_a.max_vibrs",
    );
    serde_json::from_str(&unzstd(bytes)).unwrap()
}

pub fn normalized(name: &str) -> wobblescope_core::CalibrationData {
    let mut cd = wobblescope_core::CalibrationData::from_samples(name, &load_csv(name)).unwrap();
    cd.normalize_to_frequencies();
    cd
}

/// Decompresses an embedded zstd file with the pure-Rust decoder (works on wasm32).
pub fn unzstd(bytes: &[u8]) -> String {
    use std::io::Read;
    let mut dec = structured_zstd::decoding::StreamingDecoder::new(bytes);
    let mut out = String::new();
    dec.read_to_string(&mut out).unwrap();
    out
}
