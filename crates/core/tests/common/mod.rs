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

/// Strips the capture-file suffixes (`.csv`, `.csv.zst`) from an input file name.
pub fn stem(input: &str) -> &str {
    input.trim_end_matches(".zst").trim_end_matches(".csv")
}

pub const REAL_CASES: [&str; 6] = [
    "real_x_adxl345",
    "real_x_ebb36",
    "real_x_both",
    "real_y_adxl345",
    "real_y_ebb36",
    "real_y_both",
];

pub fn load_csv(name: &str) -> RawSamples {
    if name.starts_with("ws_") || name.starts_with("win_") {
        return load_real(name);
    }
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
        "real_x_adxl345",
        "real_x_ebb36",
        "real_x_both",
        "real_y_adxl345",
        "real_y_ebb36",
        "real_y_both",
    );
    serde_json::from_str(&unzstd(bytes)).unwrap()
}

pub fn normalized(name: &str) -> wobblescope_core::CalibrationData {
    let mut cd = wobblescope_core::CalibrationData::from_samples(name, &load_csv(name)).unwrap();
    cd.normalize_to_frequencies();
    cd
}

pub fn fit_params(p: &Params) -> wobblescope_core::fit::FitParams {
    wobblescope_core::fit::FitParams {
        scv: p.scv,
        damping_ratio: p.damping_ratio,
        max_smoothing: p.max_smoothing,
        max_vibrations: p.max_vibrations,
        test_damping_ratios: p.test_damping_ratios.clone(),
        max_freq: p.max_freq,
        shapers: None,
    }
}

pub fn assert_result(label: &str, got: &wobblescope_core::fit::CalibrationResult, want: &Cand) {
    close(&format!("{label} freq"), got.freq, want.freq);
    close(&format!("{label} vibrs"), got.vibrs, want.vibrs);
    close(&format!("{label} smoothing"), got.smoothing, want.smoothing);
    close(&format!("{label} score"), got.score, want.score);
    close(&format!("{label} max_accel"), got.max_accel, want.max_accel);
}

fn rel_diff(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

/// Level 1 on every shaper's final result, then level 2 on the recommendation, falling back to
/// an accepted-and-logged level-3 near tie.
pub fn check_recommendation(label: &str, rec: &wobblescope_core::fit::Recommendation, case: &Case) {
    assert_eq!(rec.all.len(), case.ranking.len(), "{label}: ranking length");
    for (got, want) in rec.all.iter().zip(&case.ranking) {
        assert_eq!(got.name, want.name, "{label}: ranking order");
        let c = Cand {
            freq: want.freq,
            vibrs: want.vibrs,
            smoothing: want.smoothing,
            score: want.score,
            max_accel: case.fit[&want.name].fin.max_accel,
        };
        assert_result(&format!("{label} {}", want.name), got, &c);
    }
    let want = &case.best;
    let got = &rec.best;
    if got.name == want.name && (got.freq - want.freq).abs() <= 1e-6 {
        let r = &case.ranking[case.best_index];
        let fin = &case.fit[&want.name].fin;
        let c = Cand {
            freq: want.freq,
            vibrs: r.vibrs,
            smoothing: r.smoothing,
            score: r.score,
            max_accel: fin.max_accel,
        };
        assert_result(&format!("{label} best"), got, &c);
        return;
    }
    let best_score = case.ranking[case.best_index].score;
    let near = rel_diff(got.score, best_score) <= NEAR_TIE_MARGIN;
    let entry: Option<Cand> = if got.name != want.name {
        case.ranking
            .iter()
            .find(|r| r.name == got.name)
            .map(|r| Cand {
                freq: r.freq,
                vibrs: r.vibrs,
                smoothing: r.smoothing,
                score: r.score,
                max_accel: case.fit[&got.name].fin.max_accel,
            })
    } else if (got.freq - want.freq).abs() <= 0.2 + 1e-6 {
        case.fit[&got.name]
            .candidates
            .iter()
            .find(|c| (c.freq - got.freq).abs() <= 1e-6)
            .map(|c| Cand {
                freq: c.freq,
                vibrs: c.vibrs,
                smoothing: c.smoothing,
                score: c.score,
                max_accel: c.max_accel,
            })
    } else {
        None
    };
    let alternative = entry.unwrap_or_else(|| {
        panic!(
            "{label}: got {} @ {}, want {} @ {}: not an accepted alternative",
            got.name, got.freq, want.name, want.freq
        )
    });
    assert!(
        near,
        "{label}: got {} @ {} is not a near tie of {} @ {}",
        got.name, got.freq, want.name, want.freq
    );
    assert_result(&format!("{label} tie"), got, &alternative);
    log(&format!(
        "{label}: near tie accepted: got {} @ {:.1}, Klipper {} @ {:.1}",
        got.name, got.freq, want.name, want.freq
    ));
}

/// Decompresses an embedded zstd file with the pure-Rust decoder (works on wasm32).
pub fn unzstd(bytes: &[u8]) -> String {
    use std::io::Read;
    let mut dec = structured_zstd::decoding::StreamingDecoder::new(bytes);
    let mut out = String::new();
    dec.read_to_string(&mut out).unwrap();
    out
}

pub fn parse_csv(text: &str) -> RawSamples {
    let mut s = RawSamples::default();
    for line in text
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

/// A real capture from `tests/data/real`, stored as `<name>.csv.zst`.
pub fn load_real(name: &str) -> RawSamples {
    let bytes: &[u8] = match name {
        "ws_x_adxl345" => include_bytes!("../data/real/ws_x_adxl345.csv.zst"),
        "ws_x_ebb36" => include_bytes!("../data/real/ws_x_ebb36.csv.zst"),
        "ws_y_adxl345" => include_bytes!("../data/real/ws_y_adxl345.csv.zst"),
        "ws_y_ebb36" => include_bytes!("../data/real/ws_y_ebb36.csv.zst"),
        "win_x_short_adxl345" => include_bytes!("../data/real/win_x_short_adxl345.csv.zst"),
        "win_x_short_klipper_adxl345" => {
            include_bytes!("../data/real/win_x_short_klipper_adxl345.csv.zst")
        }
        "win_x_short_ebb36" => include_bytes!("../data/real/win_x_short_ebb36.csv.zst"),
        "win_x_short_klipper_ebb36" => {
            include_bytes!("../data/real/win_x_short_klipper_ebb36.csv.zst")
        }
        "win_y_short_adxl345" => include_bytes!("../data/real/win_y_short_adxl345.csv.zst"),
        "win_y_short_klipper_adxl345" => {
            include_bytes!("../data/real/win_y_short_klipper_adxl345.csv.zst")
        }
        "win_y_short_ebb36" => include_bytes!("../data/real/win_y_short_ebb36.csv.zst"),
        "win_y_short_klipper_ebb36" => {
            include_bytes!("../data/real/win_y_short_klipper_ebb36.csv.zst")
        }
        "win_x_same_adxl345" => include_bytes!("../data/real/win_x_same_adxl345.csv.zst"),
        "win_x_same_klipper_adxl345" => {
            include_bytes!("../data/real/win_x_same_klipper_adxl345.csv.zst")
        }
        "win_x_same_ebb36" => include_bytes!("../data/real/win_x_same_ebb36.csv.zst"),
        "win_x_same_klipper_ebb36" => {
            include_bytes!("../data/real/win_x_same_klipper_ebb36.csv.zst")
        }
        "win_y_same_adxl345" => include_bytes!("../data/real/win_y_same_adxl345.csv.zst"),
        "win_y_same_klipper_adxl345" => {
            include_bytes!("../data/real/win_y_same_klipper_adxl345.csv.zst")
        }
        "win_y_same_ebb36" => include_bytes!("../data/real/win_y_same_ebb36.csv.zst"),
        "win_y_same_klipper_ebb36" => {
            include_bytes!("../data/real/win_y_same_klipper_ebb36.csv.zst")
        }
        _ => panic!("unknown real capture {name}"),
    };
    parse_csv(&unzstd(bytes))
}

/// `print_time` markers recorded with a whole-window capture (`win_<case>_*`).
pub struct WindowMarkers {
    /// End of the positioning move, read right after `M400`.
    pub p1_move_end: f64,
    /// End of the sweep.
    pub p2_sweep_end: f64,
}

pub const WINDOW_CASES: [&str; 4] = ["x_short", "y_short", "x_same", "y_same"];

pub fn window_markers(case: &str) -> WindowMarkers {
    let text = match case {
        "x_short" => include_str!("../data/real/win_x_short.markers.json"),
        "y_short" => include_str!("../data/real/win_y_short.markers.json"),
        "x_same" => include_str!("../data/real/win_x_same.markers.json"),
        "y_same" => include_str!("../data/real/win_y_same.markers.json"),
        _ => panic!("unknown window case {case}"),
    };
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    WindowMarkers {
        p1_move_end: v["p1_move_end"].as_f64().unwrap(),
        p2_sweep_end: v["p2_sweep_end"].as_f64().unwrap(),
    }
}

/// Sum of the three per-axis variances over the samples with timestamps in `range`.
pub fn energy_between(s: &RawSamples, range: std::ops::Range<f64>) -> f64 {
    let idx: Vec<usize> = (0..s.len()).filter(|&i| range.contains(&s.t[i])).collect();
    let var = |v: &[f64]| {
        let m = idx.iter().map(|&i| v[i]).sum::<f64>() / idx.len() as f64;
        idx.iter().map(|&i| (v[i] - m).powi(2)).sum::<f64>() / idx.len() as f64
    };
    var(&s.x) + var(&s.y) + var(&s.z)
}

#[derive(Deserialize)]
pub struct OraclePeak {
    pub freq: f64,
    pub level: f64,
    pub above_effect: bool,
}

#[derive(Deserialize)]
pub struct OracleDataset {
    pub name: String,
    pub fr: f64,
    pub zeta: Option<f64>,
    pub peaks: Vec<OraclePeak>,
    pub detect_threshold: f64,
    pub effect_threshold: f64,
}

#[derive(Deserialize)]
pub struct OraclePerformance {
    pub name: Option<String>,
    pub shown: bool,
}

#[derive(Deserialize)]
pub struct OracleCase {
    pub datasets: Vec<OracleDataset>,
    pub performance: OraclePerformance,
}

#[derive(Deserialize)]
pub struct OracleEdge {
    pub name: String,
    pub freq_bins: Vec<f64>,
    pub psd: Vec<f64>,
    pub peaks: Vec<OraclePeak>,
    pub detect_threshold: f64,
    pub effect_threshold: f64,
}

/// Shake&Tune's own results for the golden inputs; see `golden/shaketune.py`.
#[derive(Deserialize)]
pub struct Oracle {
    pub pin: String,
    pub cases: BTreeMap<String, OracleCase>,
    pub edge_cases: Vec<OracleEdge>,
}

pub fn shaketune() -> Oracle {
    serde_json::from_str(include_str!("../golden/shaketune.json")).unwrap()
}
