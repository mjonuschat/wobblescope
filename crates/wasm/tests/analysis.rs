//! The binding's analysis must equal `core`'s own on the same inputs, natively and on wasm32.

#[macro_use]
#[path = "../../session/tests/common/mod.rs"]
mod common;

use common::{real, window_markers_x_short};
use wobblescope_core::RawSamples;
use wobblescope_core::belts::compare_psds;
use wobblescope_core::belts_run::{BeltsAnalysis, analyze_belts};
use wobblescope_core::direction::Direction;
use wobblescope_core::eligibility::{Coverage, DatasetInput, Decision, Health};
use wobblescope_core::fit::{Axis, FitParams};
use wobblescope_core::multichip::analyze;
use wobblescope_core::psd::CalibrationData;
use wobblescope_core::spectrogram::spectrogram_of;
use wobblescope_session::bridge::AxesMapChip;
use wobblescope_session::codec::encode_samples;
use wobblescope_session::meta::{
    AXES_MAP_DIRECTIONS, AxesMapRun, ChipMeta, Outcome, ProbePoint, SegmentFile, SegmentMeta,
    SessionMeta, Source, SweepSettings, TimeDomain, TimeWindow, sample_file_name,
};
use wobblescope_wasm::{AnalyzeError, Session, config_snippet};

struct Win {
    adxl: RawSamples,
    ebb: RawSamples,
    window: std::ops::Range<f64>,
}

fn win() -> Win {
    let m = window_markers_x_short();
    Win {
        adxl: real("win_x_short_adxl345"),
        ebb: real("win_x_short_ebb36"),
        window: m.p1_move_end..m.p2_sweep_end,
    }
}

fn chip(name: &str, health: Health) -> ChipMeta {
    ChipMeta {
        name: name.to_owned(),
        chip_type: None,
        nominal_rate_hz: None,
        observed_rate_hz: None,
        axes_map: None,
        mounting: None,
        health,
        health_notes: vec![],
    }
}

fn entry(chip: u32, count: usize) -> SegmentFile {
    SegmentFile {
        chip,
        sample_file: sample_file_name(chip as usize, 0),
        sample_count: count as u32,
        coverage: Coverage::Complete,
        health: None,
        health_notes: Vec::new(),
    }
}

/// The two real chips of `win_x_short` as one X segment bracketed by the capture's markers.
fn two_chip_x_meta(w: &Win, ebb_health: Health) -> SessionMeta {
    SessionMeta {
        format_version: 1,
        id: "wasm-parity".to_owned(),
        created_at: "2026-10-31T19:00:00Z".to_owned(),
        source: Source::Capture,
        time_domain: TimeDomain::KlipperPrintTime,
        axes_map_run: None,
        vibrations_run: None,
        mode: None,
        settings: None,
        printer: None,
        outcome: Outcome::Complete,
        notes: vec![],
        probe_points: vec![ProbePoint { position: None }],
        chips: vec![
            chip("adxl345", Health::Ok),
            chip("lis2dw ebb36", ebb_health),
        ],
        segments: vec![SegmentMeta {
            axis: Axis::X.into(),
            point: 0,
            window: Some(TimeWindow {
                start: w.window.start,
                end: w.window.end,
            }),
            files: vec![entry(0, w.adxl.len()), entry(1, w.ebb.len())],
            steps: vec![],
        }],
    }
}

fn direct(w: &Win) -> Vec<DatasetInput> {
    let input = |chip: &str, samples: &RawSamples| DatasetInput {
        chip: chip.to_owned(),
        point: 0,
        samples: samples.clone(),
        health: Health::Ok,
        coverage: Coverage::Complete,
        include_override: false,
        window: Some(w.window.clone()),
        exclusions: Vec::new(),
        load_error: None,
    };
    vec![input("adxl345", &w.adxl), input("lis2dw ebb36", &w.ebb)]
}

fn loaded(w: &Win, meta: SessionMeta) -> Session {
    let mut s = Session::new(meta).unwrap();
    s.add_file("c0-s0.zst", encode_samples(&w.adxl).unwrap());
    s.add_file("c1-s0.zst", encode_samples(&w.ebb).unwrap());
    s
}

parity_test! {
    fn analysis_through_the_binding_equals_direct_analysis() {
        let w = win();
        let s = loaded(&w, two_chip_x_meta(&w, Health::Ok));
        let via = s.analyze(Axis::X, &FitParams::default(), &[]).unwrap();
        let direct = analyze(&direct(&w), &FitParams::default()).unwrap();
        assert_eq!(via, direct);
        assert!(via.worst_case.is_some());
    }
}

fn sweep_to(freq_end: f64) -> SweepSettings {
    SweepSettings {
        freq_start: 5.0,
        freq_end,
        hz_per_sec: 1.0,
        accel_per_hz: 60.0,
        sweeping_accel: None,
        sweeping_period: None,
    }
}

parity_test! {
    fn the_cutoff_and_the_shaper_ceiling_follow_the_end_of_the_recorded_sweep() {
        let w = win();
        let mut meta = two_chip_x_meta(&w, Health::Ok);
        meta.settings = Some(sweep_to(250.0));
        let s = loaded(&w, meta);
        let via = s.analyze(Axis::X, &FitParams::default(), &[]).unwrap();
        let wide = FitParams {
            max_freq: Some(375.0),
            max_shaper_freq: Some(250.0),
            ..FitParams::default()
        };
        assert_eq!(via, analyze(&direct(&w), &wide).unwrap());
        assert_ne!(via, analyze(&direct(&w), &FitParams::default()).unwrap());
    }
}

parity_test! {
    fn a_short_sweep_keeps_the_default_cutoff() {
        let w = win();
        let mut meta = two_chip_x_meta(&w, Health::Ok);
        meta.settings = Some(sweep_to(100.0));
        let s = loaded(&w, meta);
        let via = s.analyze(Axis::X, &FitParams::default(), &[]).unwrap();
        assert_eq!(via, analyze(&direct(&w), &FitParams::default()).unwrap());
    }
}

parity_test! {
    fn a_cutoff_the_caller_sets_replaces_the_sweep_cutoff() {
        let w = win();
        let mut meta = two_chip_x_meta(&w, Health::Ok);
        meta.settings = Some(sweep_to(250.0));
        let s = loaded(&w, meta);
        let params = FitParams { max_freq: Some(200.0), ..FitParams::default() };
        let via = s.analyze(Axis::X, &params, &[]).unwrap();
        let expected = FitParams {
            max_freq: Some(200.0),
            max_shaper_freq: Some(250.0),
            ..FitParams::default()
        };
        assert_eq!(via, analyze(&direct(&w), &expected).unwrap());
    }
}

parity_test! {
    fn an_override_brings_an_excluded_chip_into_the_analysis_and_says_so() {
        let w = win();
        let s = loaded(&w, two_chip_x_meta(&w, Health::Degraded));
        let without = s.analyze(Axis::X, &FitParams::default(), &[]).unwrap();
        assert_eq!(without.eligibility.decisions[1].decision, Decision::Excluded);
        assert!(without.per_chip[1].recommendation.is_none());
        let with = s
            .analyze(Axis::X, &FitParams::default(), &["lis2dw ebb36".to_owned()])
            .unwrap();
        assert_eq!(with.eligibility.decisions[1].decision, Decision::Overridden);
        assert!(with.per_chip[1].recommendation.is_some());
    }
}

parity_test! {
    fn an_axis_without_segments_is_an_empty_analysis_not_an_error() {
        let w = win();
        let s = loaded(&w, two_chip_x_meta(&w, Health::Ok));
        let a = s.analyze(Axis::Y, &FitParams::default(), &[]).unwrap();
        assert!(a.per_chip.is_empty());
        assert!(a.worst_case.is_none());
    }
}

parity_test! {
    fn a_missing_file_is_named_and_nothing_is_decoded() {
        let w = win();
        let mut s = Session::new(two_chip_x_meta(&w, Health::Ok)).unwrap();
        s.add_file("c0-s0.zst", encode_samples(&w.adxl).unwrap());
        let e = s.analyze(Axis::X, &FitParams::default(), &[]).unwrap_err().to_string();
        assert!(e.contains("c1-s0.zst"), "{e}");
    }
}

parity_test! {
    fn truncated_bytes_drop_the_recording_with_the_session_crates_message() {
        let w = win();
        let mut s = Session::new(two_chip_x_meta(&w, Health::Ok)).unwrap();
        s.add_file("c0-s0.zst", encode_samples(&w.adxl).unwrap());
        let mut bytes = encode_samples(&w.ebb).unwrap();
        bytes.truncate(bytes.len() / 2);
        s.add_file("c1-s0.zst", bytes);
        let a = s.analyze(Axis::X, &FitParams::default(), &[]).unwrap();
        let e = a.eligibility.decisions[1].dropped.clone().unwrap_or_default();
        assert!(e.contains("truncated") || e.contains("corrupt"), "{e}");
        assert!(a.per_chip[0].recommendation.is_some());
        assert!(a.worst_case.is_some());
    }
}

parity_test! {
    fn a_sample_count_that_differs_from_the_metadata_drops_the_recording_naming_both_counts() {
        let w = win();
        let mut meta = two_chip_x_meta(&w, Health::Ok);
        meta.segments[0].files[1].sample_count += 1;
        let expected = meta.segments[0].files[1].sample_count;
        let s = loaded(&w, meta);
        let a = s.analyze(Axis::X, &FitParams::default(), &[]).unwrap();
        let e = a.eligibility.decisions[1].dropped.clone().unwrap_or_default();
        assert!(e.contains("c1-s0.zst"), "{e}");
        assert!(e.contains(&expected.to_string()) && e.contains(&w.ebb.len().to_string()), "{e}");
        assert!(a.worst_case.is_some());
    }
}

parity_test! {
    fn a_bad_fit_parameter_carries_cores_message() {
        let w = win();
        let s = loaded(&w, two_chip_x_meta(&w, Health::Ok));
        let params = FitParams { damping_ratio: Some(1.5), ..FitParams::default() };
        let e = s.analyze(Axis::X, &params, &[]).unwrap_err().to_string();
        assert!(e.contains("damping_ratio"), "{e}");
    }
}

parity_test! {
    fn the_snippet_equals_cores_for_the_worst_case() {
        let w = win();
        let s = loaded(&w, two_chip_x_meta(&w, Health::Ok));
        let a = s.analyze(Axis::X, &FitParams::default(), &[]).unwrap();
        let best = a.worst_case.unwrap();
        let snippet = config_snippet(&best, Axis::X, "all chips");
        assert_eq!(snippet, best.config_snippet(Axis::X, "all chips"));
        assert!(snippet.starts_with("[input_shaper]"));
    }
}

#[cfg(target_arch = "wasm32")]
mod js {
    use super::*;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_test::wasm_bindgen_test;
    use wobblescope_core::multichip::Analysis;
    use wobblescope_wasm::js::WasmSession;

    fn to_js<T: serde::Serialize>(v: &T) -> JsValue {
        v.serialize(&serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true))
            .unwrap()
    }

    #[wasm_bindgen_test]
    fn js_values_in_and_a_js_value_out_equal_the_direct_analysis() {
        let w = win();
        let mut s = WasmSession::new(to_js(&two_chip_x_meta(&w, Health::Ok))).unwrap();
        s.add_file("c0-s0.zst".to_owned(), encode_samples(&w.adxl).unwrap());
        s.add_file("c1-s0.zst".to_owned(), encode_samples(&w.ebb).unwrap());
        // Only one setting is given; every other field takes its default.
        let params = js_sys::eval("({ scv: 5, max_smoothing: 0.2 })").unwrap();
        let out = s.analyze(to_js(&Axis::X), params, None).unwrap();
        let got: Analysis = serde_wasm_bindgen::from_value(out).unwrap();
        let want = analyze(
            &direct(&w),
            &FitParams {
                max_smoothing: Some(0.2),
                ..FitParams::default()
            },
        )
        .unwrap();
        assert_eq!(got, want);
    }

    #[wasm_bindgen_test]
    fn belts_through_js_equal_the_direct_analysis() {
        let w = win();
        let mut s = WasmSession::new(to_js(&belt_meta(&w, true))).unwrap();
        s.add_file("c0-s0.zst".to_owned(), encode_samples(&w.adxl).unwrap());
        s.add_file("c0-s1.zst".to_owned(), encode_samples(&w.ebb).unwrap());
        let out = s.belts("adxl345".to_owned(), 200.0, false).unwrap();
        let got: BeltsAnalysis = serde_wasm_bindgen::from_value(out).unwrap();
        assert_eq!(got, belt_direct(&w));
    }

    #[wasm_bindgen_test]
    fn an_absent_recommendation_reaches_js_as_null() {
        let w = win();
        // Both chips degraded, so nothing is eligible.
        let mut meta = two_chip_x_meta(&w, Health::Degraded);
        meta.chips[0].health = Health::Degraded;
        let mut s = WasmSession::new(to_js(&meta)).unwrap();
        s.add_file("c0-s0.zst".to_owned(), encode_samples(&w.adxl).unwrap());
        s.add_file("c1-s0.zst".to_owned(), encode_samples(&w.ebb).unwrap());
        let out = s
            .analyze(to_js(&Axis::X), to_js(&FitParams::default()), None)
            .unwrap();
        let worst = js_sys::Reflect::get(&out, &"worst_case".into()).unwrap();
        assert!(worst.is_null(), "worst_case must be null, not undefined");
    }

    #[wasm_bindgen_test]
    fn metadata_without_chips_throws_an_error_naming_the_field() {
        let e: JsValue = WasmSession::new(js_sys::eval("({ format_version: 1 })").unwrap())
            .err()
            .expect("must fail")
            .into();
        let msg: String = js_sys::Reflect::get(&e, &"message".into())
            .unwrap()
            .as_string()
            .unwrap();
        assert!(
            msg.contains("chips") || msg.contains("missing field"),
            "{msg}"
        );
    }
}

parity_test! {
    fn a_spectrogram_through_the_session_equals_direct_computation() {
        let w = win();
        let s = loaded(&w, two_chip_x_meta(&w, Health::Ok));
        let via = s.spectrogram(Axis::X, "lis2dw ebb36", 0, 200.0).unwrap();
        assert_eq!(via, spectrogram_of(&direct(&w)[1], 200.0).unwrap());
        assert!(via.power.len() > 1);
    }
}

parity_test! {
    fn an_unknown_chip_or_point_is_named() {
        let w = win();
        let s = loaded(&w, two_chip_x_meta(&w, Health::Ok));
        for (chip, point) in [("nope", 0), ("adxl345", 1)] {
            match s.spectrogram(Axis::X, chip, point, 200.0) {
                Err(AnalyzeError::NoSuchDataset { chip: c, point: p }) => {
                    assert_eq!((c.as_str(), p), (chip, point));
                }
                other => panic!("expected NoSuchDataset, got {other:?}"),
            }
        }
    }
}

parity_test! {
    fn a_missing_file_is_named_for_a_spectrogram() {
        let w = win();
        let mut s = Session::new(two_chip_x_meta(&w, Health::Ok)).unwrap();
        s.add_file("c0-s0.zst", encode_samples(&w.adxl).unwrap());
        assert!(matches!(
            s.spectrogram(Axis::X, "adxl345", 0, 200.0),
            Err(AnalyzeError::MissingFile(name)) if name == "c1-s0.zst"
        ));
    }
}

parity_test! {
    fn a_degraded_chip_still_gets_a_map() {
        let w = win();
        let s = loaded(&w, two_chip_x_meta(&w, Health::Degraded));
        assert!(s.spectrogram(Axis::X, "lis2dw ebb36", 0, 200.0).is_ok());
    }
}

/// One chip, `adxl345`, with the ADXL recording as belt A and the EBB recording as belt B, both
/// bracketed by the capture's window. The two files differ in rate, which the comparison allows.
fn belt_meta(w: &Win, with_b: bool) -> SessionMeta {
    let window = Some(TimeWindow {
        start: w.window.start,
        end: w.window.end,
    });
    let file = |segment, count| SegmentFile {
        sample_file: sample_file_name(0, segment),
        ..entry(0, count)
    };
    let mut segments = vec![SegmentMeta {
        axis: Direction::BELT_A,
        point: 0,
        window,
        files: vec![file(0, w.adxl.len())],
        steps: vec![],
    }];
    if with_b {
        segments.push(SegmentMeta {
            axis: Direction::BELT_B,
            point: 0,
            window,
            files: vec![file(1, w.ebb.len())],
            steps: vec![],
        });
    }
    SessionMeta {
        format_version: 2,
        chips: vec![chip("adxl345", Health::Ok)],
        segments,
        ..two_chip_x_meta(w, Health::Ok)
    }
}

fn belt_session(w: &Win, with_b: bool) -> Session {
    let mut s = Session::new(belt_meta(w, with_b)).unwrap();
    s.add_file("c0-s0.zst", encode_samples(&w.adxl).unwrap());
    if with_b {
        s.add_file("c0-s1.zst", encode_samples(&w.ebb).unwrap());
    }
    s
}

fn belt_direct(w: &Win) -> BeltsAnalysis {
    let input = |samples: &RawSamples| DatasetInput {
        samples: samples.clone(),
        ..direct(w).remove(0)
    };
    analyze_belts(
        "adxl345",
        0,
        Some(&input(&w.adxl)),
        Some(&input(&w.ebb)),
        200.0,
        None,
    )
}

parity_test! {
    fn belts_read_the_similarity_over_the_recorded_sweep_band() {
        let w = win();
        let mut meta = belt_meta(&w, true);
        meta.settings = Some(SweepSettings {
            freq_start: 20.0,
            freq_end: 60.0,
            hz_per_sec: 1.0,
            accel_per_hz: 60.0,
            sweeping_accel: None,
            sweeping_period: None,
        });
        let mut s = Session::new(meta).unwrap();
        s.add_file("c0-s0.zst", encode_samples(&w.adxl).unwrap());
        s.add_file("c0-s1.zst", encode_samples(&w.ebb).unwrap());
        let got = s.belts("adxl345", 200.0, false).unwrap();
        let whole = belt_direct(&w).result.unwrap();
        let result = got.result.unwrap();
        assert_ne!(result.similarity, whole.similarity);
        assert_eq!(result.freqs, whole.freqs);
    }
}

parity_test! {
    fn belts_matches_compare_psds_on_the_same_recordings() {
        let w = win();
        let via = belt_session(&w, true).belts("adxl345", 200.0, false).unwrap();
        assert_eq!(via, belt_direct(&w));
        let result = via.result.as_ref().expect("both belts usable");
        let psd = |s: &RawSamples| {
            let cd = CalibrationData::from_samples("x", &s.slice(s.window(w.window.clone()).unwrap()))
                .unwrap();
            (cd.freq_bins, cd.psd_sum)
        };
        let ((fa, pa), (fb, pb)) = (psd(&w.adxl), psd(&w.ebb));
        assert_eq!(result, &compare_psds(&fa, &pa, &fb, &pb, 200.0));
    }
}

parity_test! {
    fn a_belt_the_chip_lacks_is_absent_not_an_error() {
        let w = win();
        let got = belt_session(&w, false).belts("adxl345", 200.0, false).unwrap();
        assert!(got.belt_a.spectrum.is_some());
        assert!(!got.belt_b.present && got.result.is_none());
    }
}

parity_test! {
    fn a_missing_belt_file_is_named() {
        let w = win();
        let mut s = Session::new(belt_meta(&w, true)).unwrap();
        s.add_file("c0-s0.zst", encode_samples(&w.adxl).unwrap());
        assert!(matches!(
            s.belts("adxl345", 200.0, false),
            Err(AnalyzeError::MissingFile(name)) if name == "c0-s1.zst"
        ));
    }
}

parity_test! {
    fn flat_belts_reach_the_caller_with_a_zero_similarity() {
        let w = win();
        let flat = |len: usize| {
            let mut s = RawSamples::default();
            for k in 0..len {
                s.t.push(w.window.start + k as f64 * 0.0005);
                s.x.push(0.0);
                s.y.push(0.0);
                s.z.push(0.0);
            }
            s
        };
        let (a, b) = (flat(8000), flat(8000));
        let mut meta = belt_meta(&w, true);
        meta.segments[0].files[0].sample_count = a.len() as u32;
        meta.segments[1].files[0].sample_count = b.len() as u32;
        let mut s = Session::new(meta).unwrap();
        s.add_file("c0-s0.zst", encode_samples(&a).unwrap());
        s.add_file("c0-s1.zst", encode_samples(&b).unwrap());
        let got = s.belts("adxl345", 200.0, false).unwrap();
        assert_eq!(got.result.unwrap().similarity, 0.0);
    }
}

// ── axes-map helpers ────────────────────────────────────────────────────────

/// Sample rate used for synthetic axes-map recordings.
const AXES_MAP_RATE: f64 = 3200.0;
/// Peak velocity (mm/s) a healthy segment sees. Above the analysis floor.
const AXES_MAP_ABOVE: f64 = 80.0;

/// A trapezoid velocity move: `peak` mm/s on chip axis `axis`. Pass `None` for noise only.
/// Gravity on chip Z, deterministic noise on every axis.
///
/// Copied from the approach in `crates/core/src/axes_map/bed.rs` tests. Not imported
/// because that helper is private to its crate.
fn axes_map_trapezoid(axis: Option<usize>, peak: f64) -> RawSamples {
    let n = (0.6 * AXES_MAP_RATE) as usize;
    let ramp = 0.1;
    let mut cols = [vec![0.0; n], vec![0.0; n], vec![9810.0; n]];
    let mut state = 12345_u32;
    for i in 0..n {
        let t = i as f64 / AXES_MAP_RATE;
        for col in &mut cols {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            col[i] += (f64::from(state >> 16) / 65536.0 - 0.5) * 100.0;
        }
        if let Some(k) = axis {
            let a = peak / ramp;
            cols[k][i] += if t < ramp {
                a
            } else if t > 0.3 && t < 0.3 + ramp {
                -a
            } else {
                0.0
            };
        }
    }
    let [x, y, z] = cols;
    RawSamples {
        t: (0..n).map(|i| i as f64 / AXES_MAP_RATE).collect(),
        x,
        y,
        z,
    }
}

/// A minimal axes-map chip with the given name and health.
fn am_chip(name: &str, health: Health, notes: Vec<String>) -> ChipMeta {
    ChipMeta {
        name: name.to_owned(),
        chip_type: None,
        nominal_rate_hz: None,
        observed_rate_hz: None,
        axes_map: None,
        mounting: None,
        health,
        health_notes: notes,
    }
}

/// A `SegmentFile` entry with complete coverage and the standard file name.
fn am_file(chip: u32, segment: usize, count: usize) -> SegmentFile {
    SegmentFile {
        chip,
        sample_file: sample_file_name(chip as usize, segment),
        sample_count: count as u32,
        coverage: Coverage::Complete,
        health: None,
        health_notes: Vec::new(),
    }
}

/// Axes-map run settings that match the AXES_MAP_ABOVE peak (speed 80 mm/s, accel 3000 mm/s²).
fn am_run() -> AxesMapRun {
    AxesMapRun {
        max_velocity: None,
        speed: 80.0,
        accel: 3000.0,
        z_height: 30.0,
        travel_speed: 150.0,
        max_z_velocity: Some(100.0),
        max_z_accel: Some(3000.0),
    }
}

/// A valid axes-map `SessionMeta` with `chips.len()` chips and three full segments.
/// `counts[chip][segment]` is the sample count for that file. Pass `None` to omit the file.
fn axes_map_meta(
    chips: Vec<ChipMeta>,
    counts: Vec<[Option<usize>; 3]>,
    segments: usize,
) -> SessionMeta {
    let n_chips = chips.len();
    SessionMeta {
        format_version: 2,
        id: "am-wasm-test".to_owned(),
        created_at: "2026-11-12T00:00:00Z".to_owned(),
        source: Source::Capture,
        time_domain: TimeDomain::KlipperPrintTime,
        axes_map_run: Some(am_run()),
        vibrations_run: None,
        mode: None,
        settings: None,
        printer: None,
        outcome: Outcome::Complete,
        notes: vec![],
        probe_points: (0..3).map(|_| ProbePoint { position: None }).collect(),
        chips,
        segments: (0..segments)
            .map(|s| SegmentMeta {
                axis: AXES_MAP_DIRECTIONS[s],
                point: s as u32,
                window: None,
                files: (0..n_chips)
                    .filter_map(|c| {
                        counts
                            .get(c)
                            .and_then(|row| row[s])
                            .map(|n| am_file(c as u32, s, n))
                    })
                    .collect(),
                steps: vec![],
            })
            .collect(),
    }
}

/// Three-sweep recordings for chip `c` as encoded bytes. Axes 0, 1, 2 each in turn.
fn am_encoded_segs(_c: usize) -> [Vec<u8>; 3] {
    std::array::from_fn(|seg| {
        encode_samples(&axes_map_trapezoid(Some(seg), AXES_MAP_ABOVE)).unwrap()
    })
}

/// Sample count produced by `axes_map_trapezoid`.
fn am_n() -> usize {
    (0.6 * AXES_MAP_RATE) as usize
}

/// Creates a `Session` with one chip and all three recordings loaded.
fn am_loaded_1chip() -> Session {
    let n = am_n();
    let chip = am_chip("adxl345", Health::Ok, vec![]);
    let meta = axes_map_meta(vec![chip], vec![[Some(n), Some(n), Some(n)]], 3);
    let mut s = Session::new(meta).unwrap();
    for (seg, bytes) in am_encoded_segs(0).into_iter().enumerate() {
        s.add_file(&sample_file_name(0, seg), bytes);
    }
    s
}

// ── axes-map tests ───────────────────────────────────────────────────────────

parity_test! {
    fn a_calibration_session_analyses_every_chip() {
        let s = am_loaded_1chip();
        let chips = s.analyze_axes_map().unwrap();
        assert_eq!(chips.len(), 1);
        assert!(
            matches!(&chips[0], AxesMapChip::Analysed { chip, .. } if chip == "adxl345"),
            "expected Analysed, got {chips:?}",
        );
    }
}

parity_test! {
    fn an_incomplete_chip_is_reported_not_analysed() {
        // Session with only two segments: Z sweep is absent.
        let n = am_n();
        let chip = am_chip("adxl345", Health::Ok, vec![]);
        let meta = axes_map_meta(vec![chip], vec![[Some(n), Some(n), Some(n)]], 2);
        let mut s = Session::new(meta).unwrap();
        for seg in 0..2 {
            s.add_file(&sample_file_name(0, seg), am_encoded_segs(0)[seg].clone());
        }
        let chips = s.analyze_axes_map().unwrap();
        assert_eq!(chips.len(), 1);
        assert!(
            matches!(&chips[0], AxesMapChip::Incomplete { missing, .. } if missing.contains('Z')),
            "expected Incomplete naming Z, got {chips:?}",
        );
    }
}

parity_test! {
    fn a_missing_sample_file_makes_only_that_chip_incomplete() {
        let n = am_n();
        let chips_meta = vec![
            am_chip("adxl345", Health::Ok, vec![]),
            am_chip("lis2dw", Health::Ok, vec![]),
        ];
        let counts = vec![
            [Some(n), Some(n), Some(n)],
            [Some(n), Some(n), Some(n)],
        ];
        let meta = axes_map_meta(chips_meta, counts, 3);
        let mut s = Session::new(meta).unwrap();
        // Load all files for chip 0 but omit chip 1's first segment.
        for seg in 0..3 {
            s.add_file(&sample_file_name(0, seg), am_encoded_segs(0)[seg].clone());
        }
        for seg in 1..3 {
            s.add_file(&sample_file_name(1, seg), am_encoded_segs(1)[seg].clone());
        }
        // c1-s0.zst is never added.
        let chips = s.analyze_axes_map().unwrap();
        assert_eq!(chips.len(), 2);
        assert!(
            matches!(&chips[0], AxesMapChip::Analysed { chip, .. } if chip == "adxl345"),
            "chip 0 should be Analysed, got {chips:?}",
        );
        assert!(
            matches!(&chips[1], AxesMapChip::Incomplete { missing, .. } if missing.contains("c1-s0.zst")),
            "chip 1 should be Incomplete naming c1-s0.zst, got {chips:?}",
        );
    }
}

parity_test! {
    fn a_degraded_chip_is_analysed_with_its_notes() {
        let n = am_n();
        let note = "mounting screw loose".to_owned();
        let chip = am_chip("adxl345", Health::Degraded, vec![note.clone()]);
        let meta = axes_map_meta(vec![chip], vec![[Some(n), Some(n), Some(n)]], 3);
        let mut s = Session::new(meta).unwrap();
        for (seg, bytes) in am_encoded_segs(0).into_iter().enumerate() {
            s.add_file(&sample_file_name(0, seg), bytes);
        }
        let chips = s.analyze_axes_map().unwrap();
        assert_eq!(chips.len(), 1);
        let AxesMapChip::Analysed { health, health_notes, .. } = &chips[0] else {
            panic!("expected Analysed, got {chips:?}");
        };
        assert_eq!(*health, Health::Degraded);
        assert!(health_notes.contains(&note), "notes: {health_notes:?}");
    }
}

parity_test! {
    fn a_chip_with_too_few_samples_fails_alone() {
        let n = am_n();
        // Chip 1 has only 5 samples per segment, below MIN_SAMPLES=10.
        let tiny = 5_usize;
        let chips_meta = vec![
            am_chip("adxl345", Health::Ok, vec![]),
            am_chip("lis2dw", Health::Ok, vec![]),
        ];
        let counts = vec![
            [Some(n), Some(n), Some(n)],
            [Some(tiny), Some(tiny), Some(tiny)],
        ];
        let meta = axes_map_meta(chips_meta, counts, 3);
        let mut s = Session::new(meta).unwrap();
        for seg in 0..3 {
            s.add_file(&sample_file_name(0, seg), am_encoded_segs(0)[seg].clone());
        }
        for seg in 0..3 {
            let tiny_data = RawSamples {
                t: (0..tiny).map(|i| i as f64 / AXES_MAP_RATE).collect(),
                x: vec![0.0; tiny],
                y: vec![0.0; tiny],
                z: vec![9810.0; tiny],
            };
            s.add_file(&sample_file_name(1, seg), encode_samples(&tiny_data).unwrap());
        }
        let chips = s.analyze_axes_map().unwrap();
        assert_eq!(chips.len(), 2);
        assert!(
            matches!(&chips[0], AxesMapChip::Analysed { chip, .. } if chip == "adxl345"),
            "chip 0 should be Analysed, got {chips:?}",
        );
        assert!(
            matches!(&chips[1], AxesMapChip::Failed { chip, .. } if chip == "lis2dw"),
            "chip 1 should be Failed, got {chips:?}",
        );
    }
}

parity_test! {
    fn low_actual_velocity_uses_recorded_toolhead_limit() {
        let n = am_n();
        let mut meta = axes_map_meta(vec![am_chip("adxl345", Health::Ok, vec![])], vec![[Some(n); 3]], 3);
        meta.axes_map_run.as_mut().unwrap().max_velocity = Some(10.0);
        let mut session = Session::new(meta).unwrap();
        for seg in 0..3 {
            let bytes = encode_samples(&axes_map_trapezoid(Some(seg), 10.0)).unwrap();
            session.add_file(&sample_file_name(0, seg), bytes);
        }
        let chips = session.analyze_axes_map().unwrap();
        assert!(matches!(&chips[0], AxesMapChip::Analysed { result, .. } if result.map.as_deref() == Some("x, y, z")), "{chips:?}");
    }
}

fn vibrations_meta(outcome: Outcome) -> SessionMeta {
    use wobblescope_session::meta::{StepMeta, VibrationsRun};
    let segment = |axis: Direction, segment: usize| SegmentMeta {
        axis,
        point: 0,
        window: Some(TimeWindow {
            start: 0.0,
            end: 4.0,
        }),
        files: vec![SegmentFile {
            chip: 0,
            sample_file: sample_file_name(0, segment),
            sample_count: 100,
            coverage: Coverage::Complete,
            health: None,
            health_notes: Vec::new(),
        }],
        steps: vec![StepMeta {
            speed: 20.0,
            window: TimeWindow {
                start: 0.0,
                end: 1.0,
            },
        }],
    };
    SessionMeta {
        format_version: 1,
        id: "vibrations".to_owned(),
        created_at: "2026-11-16T10:00:00Z".to_owned(),
        source: Source::Capture,
        time_domain: TimeDomain::KlipperPrintTime,
        mode: None,
        settings: None,
        printer: None,
        outcome,
        axes_map_run: None,
        vibrations_run: Some(VibrationsRun {
            size: 100.0,
            z_height: 20.0,
            max_speed: 50.0,
            speed_increment: 10.0,
            accel: 3000.0,
            travel_speed: 120.0,
            min_speed: 20.0,
            kinematics: "cartesian".to_owned(),
            max_velocity: None,
        }),
        notes: vec![],
        probe_points: vec![ProbePoint { position: None }],
        chips: vec![chip("adxl345", Health::Ok)],
        segments: vec![segment(Direction::X, 0), segment(Direction::Y, 1)],
    }
}

parity_test! {
    fn vibrations_reports_a_file_that_was_never_added_as_that_chips_failure() {
        let s = Session::new(vibrations_meta(Outcome::Complete)).unwrap();
        let out = s.vibrations(200.0, false).unwrap();
        assert!(out.per_chip[0].result.is_none());
        assert!(out.per_chip[0].failure.as_deref().unwrap().contains("c0-s0.zst"));
        assert!(out.worst_case.is_none());
    }
}

parity_test! {
    fn vibrations_refuses_an_incomplete_session() {
        let meta = vibrations_meta(Outcome::Incomplete { reason: "stopped".to_owned() });
        let err = Session::new(meta).unwrap().vibrations(200.0, false).unwrap_err();
        assert!(matches!(err, AnalyzeError::Session(_)));
        assert!(err.to_string().contains("incomplete"));
    }
}
