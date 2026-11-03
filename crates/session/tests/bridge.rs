#[macro_use]
mod common;

use std::collections::HashMap;

use common::{real, window_markers_x_short};
use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::{Coverage, DatasetInput, Decision, Health};
use wobblescope_core::fit::{Axis, FitParams};
use wobblescope_core::multichip::analyze;
use wobblescope_session::SessionError;
use wobblescope_session::bridge::{apply_overrides, dataset_inputs};
use wobblescope_session::codec::{decode_samples, encode_samples};
use wobblescope_session::meta::{
    ChipMeta, Outcome, ProbePoint, SegmentFile, SegmentMeta, SessionMeta, Source, TimeDomain,
    TimeWindow, sample_file_name,
};

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

fn entry(chip: u32, segment: usize, count: usize, coverage: Coverage) -> SegmentFile {
    SegmentFile {
        chip,
        sample_file: sample_file_name(chip as usize, segment),
        sample_count: count as u32,
        coverage,
        health: None,
        health_notes: Vec::new(),
    }
}

fn meta(chips: Vec<ChipMeta>, segments: Vec<SegmentMeta>) -> SessionMeta {
    SessionMeta {
        format_version: 1,
        id: "bridge".to_owned(),
        created_at: "2026-10-31T19:00:00Z".to_owned(),
        source: Source::Capture,
        time_domain: TimeDomain::KlipperPrintTime,
        mode: None,
        settings: None,
        printer: None,
        outcome: Outcome::Complete,
        notes: vec![],
        probe_points: vec![ProbePoint { position: None }],
        chips,
        segments,
    }
}

/// The two real chips of the `win_x_short` capture and the window their markers bracket.
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

/// An in-memory store: sample files are encoded and decoded like real ones.
fn loader(
    files: Vec<(&str, &RawSamples)>,
) -> impl FnMut(&SegmentFile) -> Result<RawSamples, SessionError> {
    let stored: HashMap<String, Vec<u8>> = files
        .into_iter()
        .map(|(n, s)| (n.to_owned(), encode_samples(s).unwrap()))
        .collect();
    move |f| decode_samples(&stored[&f.sample_file])
}

fn two_chip_x_meta(w: &Win, coverage: Coverage, ebb_health: Health) -> SessionMeta {
    meta(
        vec![
            chip("adxl345", Health::Ok),
            chip("lis2dw ebb36", ebb_health),
        ],
        vec![SegmentMeta {
            axis: Axis::X,
            point: 0,
            window: Some(TimeWindow {
                start: w.window.start,
                end: w.window.end,
            }),
            files: vec![
                entry(0, 0, w.adxl.len(), Coverage::Complete),
                entry(1, 0, w.ebb.len(), coverage),
            ],
        }],
    )
}

fn direct(w: &Win, coverage: Coverage, ebb_health: Health) -> Vec<DatasetInput> {
    let input = |chip: &str, samples: &RawSamples, health, coverage| DatasetInput {
        chip: chip.to_owned(),
        point: 0,
        samples: samples.clone(),
        health,
        coverage,
        include_override: false,
        window: Some(w.window.clone()),
    };
    vec![
        input("adxl345", &w.adxl, Health::Ok, Coverage::Complete),
        input("lis2dw ebb36", &w.ebb, ebb_health, coverage),
    ]
}

parity_test! {
    fn analysis_through_a_session_equals_direct_analysis() {
        let w = win();
        let m = two_chip_x_meta(&w, Coverage::Complete, Health::Ok);
        let via = dataset_inputs(
            &m,
            Axis::X,
            loader(vec![("c0-s0.zst", &w.adxl), ("c1-s0.zst", &w.ebb)]),
        )
        .unwrap();
        let params = FitParams::default();
        let a = analyze(&via, &params).unwrap();
        let b = analyze(&direct(&w, Coverage::Complete, Health::Ok), &params).unwrap();
        assert_eq!(a, b);
        assert!(a.per_chip.iter().all(|c| !c.windows.is_empty()));
    }
}

parity_test! {
    fn only_the_requested_axis_is_returned_in_segment_then_file_order() {
        let s = |n: usize| RawSamples { t: vec![0.0; n], x: vec![0.0; n], y: vec![0.0; n], z: vec![0.0; n] };
        let m = meta(
            vec![chip("a", Health::Ok), chip("b", Health::Ok)],
            vec![
                SegmentMeta {
                    axis: Axis::X,
                    point: 0,
                    window: None,
                    files: vec![entry(1, 0, 2, Coverage::Complete), entry(0, 0, 2, Coverage::Complete)],
                },
                SegmentMeta {
                    axis: Axis::Y,
                    point: 0,
                    window: None,
                    files: vec![entry(0, 1, 2, Coverage::Complete)],
                },
            ],
        );
        let samples = s(2);
        let inputs = dataset_inputs(&m, Axis::X, |_| Ok(samples.clone())).unwrap();
        let names: Vec<&str> = inputs.iter().map(|i| i.chip.as_str()).collect();
        assert_eq!(names, ["b", "a"]);
        let y = dataset_inputs(&m, Axis::Y, |_| Ok(samples.clone())).unwrap();
        assert_eq!(y.len(), 1);
        assert!(inputs.iter().all(|i| !i.include_override && i.window.is_none()));
    }
}

parity_test! {
    fn incomplete_coverage_reaches_eligibility() {
        let w = win();
        let missing = "last 3.2 s".to_owned();
        let coverage = Coverage::Incomplete { missing: missing.clone() };
        let m = two_chip_x_meta(&w, coverage, Health::Ok);
        let inputs = dataset_inputs(
            &m,
            Axis::X,
            loader(vec![("c0-s0.zst", &w.adxl), ("c1-s0.zst", &w.ebb)]),
        )
        .unwrap();
        let a = analyze(&inputs, &FitParams::default()).unwrap();
        let d = &a.eligibility.decisions[1];
        assert_eq!(d.chip, "lis2dw ebb36");
        assert_eq!(d.decision, Decision::Excluded);
        assert!(d.reasons.contains(&"incomplete coverage: last 3.2 s".to_owned()));
    }
}

parity_test! {
    fn degraded_health_reaches_eligibility() {
        let w = win();
        let m = two_chip_x_meta(&w, Coverage::Complete, Health::Degraded);
        let inputs = dataset_inputs(
            &m,
            Axis::X,
            loader(vec![("c0-s0.zst", &w.adxl), ("c1-s0.zst", &w.ebb)]),
        )
        .unwrap();
        let a = analyze(&inputs, &FitParams::default()).unwrap();
        let d = &a.eligibility.decisions[1];
        assert_eq!(d.decision, Decision::Excluded);
        assert!(d.reasons.contains(&"degraded capture health".to_owned()));
    }
}

parity_test! {
    fn an_override_includes_the_named_chip_and_leaves_the_others_alone() {
        let w = win();
        let m = two_chip_x_meta(&w, Coverage::Complete, Health::Degraded);
        let mut inputs = dataset_inputs(
            &m,
            Axis::X,
            loader(vec![("c0-s0.zst", &w.adxl), ("c1-s0.zst", &w.ebb)]),
        )
        .unwrap();
        apply_overrides(&mut inputs, &["lis2dw ebb36".to_owned(), "no such chip".to_owned()]);
        assert_eq!(
            inputs.iter().map(|i| i.include_override).collect::<Vec<_>>(),
            vec![false, true]
        );
        let a = analyze(&inputs, &FitParams::default()).unwrap();
        assert_eq!(a.eligibility.decisions[0].decision, Decision::Included);
        let d = &a.eligibility.decisions[1];
        assert_eq!(d.decision, Decision::Overridden);
        assert!(d.reasons.contains(&"degraded capture health".to_owned()));
    }
}

parity_test! {
    fn a_loaded_length_that_differs_from_the_recorded_count_is_an_error() {
        let m = meta(
            vec![chip("a", Health::Ok)],
            vec![SegmentMeta {
                axis: Axis::X,
                point: 0,
                window: None,
                files: vec![entry(0, 0, 1000, Coverage::Complete)],
            }],
        );
        let short = RawSamples { t: vec![0.0; 999], x: vec![0.0; 999], y: vec![0.0; 999], z: vec![0.0; 999] };
        let err = dataset_inputs(&m, Axis::X, |_| Ok(short.clone())).unwrap_err();
        assert!(matches!(
            err,
            SessionError::SampleCountMismatch { expected: 1000, found: 999, .. }
        ));
    }
}

parity_test! {
    fn a_loader_error_is_returned_unchanged() {
        let w = win();
        let m = two_chip_x_meta(&w, Coverage::Complete, Health::Ok);
        let err = dataset_inputs(&m, Axis::X, |_| Err(SessionError::Truncated)).unwrap_err();
        assert!(matches!(err, SessionError::Truncated));
    }
}

parity_test! {
    fn an_axis_with_no_segments_yields_nothing() {
        let w = win();
        let m = two_chip_x_meta(&w, Coverage::Complete, Health::Ok);
        let inputs = dataset_inputs(&m, Axis::Y, |_| panic!("no file should be loaded")).unwrap();
        assert!(inputs.is_empty());
    }
}
