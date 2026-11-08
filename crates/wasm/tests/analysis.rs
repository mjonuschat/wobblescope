//! The binding's analysis must equal `core`'s own on the same inputs, natively and on wasm32.

#[macro_use]
#[path = "../../session/tests/common/mod.rs"]
mod common;

use common::{real, window_markers_x_short};
use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::{Coverage, DatasetInput, Decision, Health};
use wobblescope_core::fit::{Axis, FitParams};
use wobblescope_core::multichip::analyze;
use wobblescope_session::codec::encode_samples;
use wobblescope_session::meta::{
    ChipMeta, Outcome, ProbePoint, SegmentFile, SegmentMeta, SessionMeta, Source, TimeDomain,
    TimeWindow, sample_file_name,
};
use wobblescope_wasm::{Session, config_snippet};

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
            axis: Axis::X,
            point: 0,
            window: Some(TimeWindow {
                start: w.window.start,
                end: w.window.end,
            }),
            files: vec![entry(0, w.adxl.len()), entry(1, w.ebb.len())],
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
        let snippet = config_snippet(&best, Axis::X);
        assert_eq!(snippet, best.config_snippet(Axis::X));
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
