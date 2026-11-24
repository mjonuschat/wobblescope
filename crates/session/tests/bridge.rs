#[macro_use]
mod common;

use std::collections::HashMap;

use common::{real, window_markers_x_short};
use wobblescope_core::RawSamples;
use wobblescope_core::axes_map::Mounting;
use wobblescope_core::direction::Direction;
use wobblescope_core::eligibility::{Coverage, DatasetInput, Decision, Health};
use wobblescope_core::fit::{Axis, FitParams};
use wobblescope_core::multichip::analyze;
use wobblescope_session::SessionError;
use wobblescope_session::bridge::{
    AxesMapChip, ChipCapture, apply_overrides, axes_map_inputs, axes_map_params, belt_inputs,
    dataset_inputs,
};
use wobblescope_session::codec::{decode_samples, encode_samples};
use wobblescope_session::meta::{
    AXES_MAP_DIRECTIONS, AxesMapRun, ChipMeta, Outcome, ProbePoint, SegmentFile, SegmentMeta,
    SessionMeta, Source, TimeDomain, TimeWindow, sample_file_name,
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
        axes_map_run: None,
        vibrations_run: None,
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
            axis: Axis::X.into(),
            point: 0,
            window: Some(TimeWindow {
                start: w.window.start,
                end: w.window.end,
            }),
            files: vec![
                entry(0, 0, w.adxl.len(), Coverage::Complete),
                entry(1, 0, w.ebb.len(), coverage),
            ],
            steps: vec![],
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
        exclusions: Vec::new(),
        load_error: None,
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
                    axis: Axis::X.into(),
                    point: 0,
                    window: None,
                    files: vec![entry(1, 0, 2, Coverage::Complete), entry(0, 0, 2, Coverage::Complete)],
 steps: vec![],
 },
                SegmentMeta {
                    axis: Axis::Y.into(),
                    point: 0,
                    window: None,
                    files: vec![entry(0, 1, 2, Coverage::Complete)],
 steps: vec![],
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
    fn a_loaded_length_that_differs_from_the_recorded_count_drops_the_recording() {
        let m = meta(
            vec![chip("a", Health::Ok)],
            vec![SegmentMeta {
                axis: Axis::X.into(),
                point: 0,
                window: None,
                files: vec![entry(0, 0, 1000, Coverage::Complete)],
 steps: vec![],
 }],
        );
        let short = RawSamples { t: vec![0.0; 999], x: vec![0.0; 999], y: vec![0.0; 999], z: vec![0.0; 999] };
        let inputs = dataset_inputs(&m, Axis::X, |_| Ok(short.clone())).unwrap();
        let message = inputs[0].load_error.as_deref().unwrap();
        assert!(message.contains("1000") && message.contains("999"), "{message}");
        assert!(inputs[0].samples.is_empty());
    }
}

parity_test! {
    fn a_file_that_fails_to_load_is_dropped_and_the_other_chip_analysed() {
        let w = win();
        let m = two_chip_x_meta(&w, Coverage::Complete, Health::Ok);
        let mut good = loader(vec![("c0-s0.zst", &w.adxl)]);
        let inputs = dataset_inputs(&m, Axis::X, |f| {
            if f.sample_file == "c1-s0.zst" { Err(SessionError::Truncated) } else { good(f) }
        })
        .unwrap();
        assert_eq!(inputs[1].load_error.as_deref(), Some(SessionError::Truncated.to_string().as_str()));
        let a = analyze(&inputs, &FitParams::default()).unwrap();
        assert!(a.eligibility.decisions[1].dropped.is_some());
        assert!(a.per_chip[0].recommendation.is_some());
        assert!(a.worst_case.is_some());
    }
}

parity_test! {
    fn a_metadata_rule_violation_still_fails() {
        let w = win();
        let mut m = two_chip_x_meta(&w, Coverage::Complete, Health::Ok);
        m.segments[0].point = 7;
        assert!(dataset_inputs(&m, Axis::X, |_| Ok(RawSamples::default())).is_err());
    }
}

/// One chip recorded on X and on Y at the same point, each file with its own stored health.
fn two_axis_meta(
    w: &Win,
    chip_health: Health,
    x: Option<Health>,
    y: Option<Health>,
) -> SessionMeta {
    let window = Some(TimeWindow {
        start: w.window.start,
        end: w.window.end,
    });
    let file = |segment, health| SegmentFile {
        health,
        ..entry(0, segment, w.adxl.len(), Coverage::Complete)
    };
    meta(
        vec![chip("adxl345", chip_health)],
        vec![
            SegmentMeta {
                axis: Axis::X.into(),
                point: 0,
                window,
                files: vec![file(0, x)],
                steps: vec![],
            },
            SegmentMeta {
                axis: Axis::Y.into(),
                point: 0,
                window,
                files: vec![file(1, y)],
                steps: vec![],
            },
        ],
    )
}

parity_test! {
    fn each_recording_is_judged_by_its_own_stored_health() {
        let w = win();
        let m = two_axis_meta(&w, Health::Degraded, Some(Health::Degraded), Some(Health::Ok));
        let load = || loader(vec![("c0-s0.zst", &w.adxl), ("c0-s1.zst", &w.adxl)]);
        let y = analyze(&dataset_inputs(&m, Axis::Y, load()).unwrap(), &FitParams::default()).unwrap();
        assert_eq!(y.eligibility.decisions[0].decision, Decision::Included);
        assert_eq!(y.per_chip[0].health, Health::Ok);
        assert!(y.worst_case.is_some());
        let x = analyze(&dataset_inputs(&m, Axis::X, load()).unwrap(), &FitParams::default()).unwrap();
        assert_eq!(x.eligibility.decisions[0].decision, Decision::Excluded);
        assert_eq!(x.per_chip[0].health, Health::Degraded);
    }
}

parity_test! {
    fn a_recording_without_stored_health_takes_its_chips() {
        let w = win();
        let m = two_axis_meta(&w, Health::Ok, None, None);
        let inputs = dataset_inputs(&m, Axis::X, loader(vec![("c0-s0.zst", &w.adxl)])).unwrap();
        assert_eq!(inputs[0].health, Health::Ok);
    }
}

fn rated(nominal: Option<f64>, observed: Option<f64>) -> SessionMeta {
    let w = win();
    let mut m = two_chip_x_meta(&w, Coverage::Complete, Health::Ok);
    m.chips[1].nominal_rate_hz = nominal;
    m.chips[1].observed_rate_hz = observed;
    m
}

parity_test! {
    fn an_undersampled_chip_is_excluded_and_the_override_includes_it() {
        let w = win();
        let m = rated(Some(1600.0), Some(395.0));
        let load = || loader(vec![("c0-s0.zst", &w.adxl), ("c1-s0.zst", &w.ebb)]);
        let mut inputs = dataset_inputs(&m, Axis::X, load()).unwrap();
        let a = analyze(&inputs, &FitParams::default()).unwrap();
        let d = &a.eligibility.decisions[1];
        assert_eq!(d.decision, Decision::Excluded);
        let reason = d.reasons.join("; ");
        assert!(reason.contains("395 Hz") && reason.contains("1600 Hz") && reason.contains("firmware"), "{reason}");
        apply_overrides(&mut inputs, &["lis2dw ebb36".to_owned()]);
        let a = analyze(&inputs, &FitParams::default()).unwrap();
        assert_eq!(a.eligibility.decisions[1].decision, Decision::Overridden);
        let both = analyze(&direct(&w, Coverage::Complete, Health::Ok), &FitParams::default()).unwrap();
        assert_eq!(a.worst_case, both.worst_case);
    }
}

parity_test! {
    fn no_undersampling_rule_without_both_rates_or_at_exactly_half() {
        let w = win();
        for m in [rated(None, Some(395.0)), rated(Some(1600.0), None), rated(Some(1600.0), Some(800.0))] {
            let inputs = dataset_inputs(&m, Axis::X, loader(vec![("c0-s0.zst", &w.adxl), ("c1-s0.zst", &w.ebb)])).unwrap();
            assert!(inputs[1].exclusions.is_empty(), "{:?}", m.chips[1]);
        }
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

/// Two chips recorded belt A (segment 0) and belt B (segment 1) at point 0. `b_chips` lists the
/// chips that have a belt B file.
fn belt_meta(w: &Win, b_chips: &[u32]) -> SessionMeta {
    let window = Some(TimeWindow {
        start: w.window.start,
        end: w.window.end,
    });
    let file = |chip: u32, segment| {
        let count = if chip == 0 { w.adxl.len() } else { w.ebb.len() };
        entry(chip, segment, count, Coverage::Complete)
    };
    let mut m = meta(
        vec![
            chip("adxl345", Health::Ok),
            chip("lis2dw ebb36", Health::Ok),
        ],
        vec![
            SegmentMeta {
                axis: Direction::BELT_A,
                point: 0,
                window,
                files: vec![file(0, 0), file(1, 0)],
                steps: vec![],
            },
            SegmentMeta {
                axis: Direction::BELT_B,
                point: 0,
                window,
                files: b_chips.iter().map(|&c| file(c, 1)).collect(),
                steps: vec![],
            },
        ],
    );
    m.format_version = 2;
    m
}

fn belt_files(w: &Win) -> Vec<(&'static str, &RawSamples)> {
    vec![
        ("c0-s0.zst", &w.adxl),
        ("c1-s0.zst", &w.ebb),
        ("c0-s1.zst", &w.adxl),
        ("c1-s1.zst", &w.ebb),
    ]
}

parity_test! {
    fn belt_inputs_picks_the_chips_two_segments() {
        let w = win();
        let m = belt_meta(&w, &[0, 1]);
        let (point, a, b) = belt_inputs(&m, "lis2dw ebb36", loader(belt_files(&w))).unwrap();
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_eq!(point, 0);
        assert_eq!((a.chip.as_str(), b.chip.as_str()), ("lis2dw ebb36", "lis2dw ebb36"));
        assert_eq!(a.samples, w.ebb);
        assert_eq!(b.samples, w.ebb);
        assert_eq!(a.window, Some(w.window.clone()));
    }
}

parity_test! {
    fn the_point_of_a_belt_session_is_the_shared_segment_point_or_0_without_belts() {
        let w = win();
        let mut m = belt_meta(&w, &[0, 1]);
        m.probe_points = vec![ProbePoint { position: None }; 4];
        for segment in &mut m.segments {
            segment.point = 3;
        }
        let (point, a, b) = belt_inputs(&m, "adxl345", loader(belt_files(&w))).unwrap();
        assert_eq!(point, 3);
        assert!(a.is_some() && b.is_some());
        let (point, a, b) = belt_inputs(&m, "no such chip", loader(belt_files(&w))).unwrap();
        assert_eq!(point, 3);
        assert!(a.is_none() && b.is_none());
        m.segments.clear();
        let (point, _, _) = belt_inputs(&m, "adxl345", loader(belt_files(&w))).unwrap();
        assert_eq!(point, 0);
    }
}

parity_test! {
    fn the_belt_point_comes_from_the_segments_even_for_a_chip_without_files() {
        let w = win();
        let mut m = belt_meta(&w, &[0, 1]);
        m.probe_points = vec![ProbePoint { position: None }; 4];
        for segment in &mut m.segments {
            segment.point = 3;
            segment.files.retain(|f| f.chip == 1);
        }
        let (point, a, b) = belt_inputs(&m, "adxl345", loader(belt_files(&w))).unwrap();
        assert_eq!(point, 3);
        assert!(a.is_none() && b.is_none());
    }
}

parity_test! {
    fn the_loader_is_never_called_for_another_chip() {
        let w = win();
        let m = belt_meta(&w, &[0, 1]);
        let mut asked = Vec::new();
        let mut inner = loader(belt_files(&w));
        belt_inputs(&m, "adxl345", |f| {
            asked.push(f.sample_file.clone());
            inner(f)
        })
        .unwrap();
        assert_eq!(asked, ["c0-s0.zst", "c0-s1.zst"]);
    }
}

parity_test! {
    fn a_chip_missing_from_belt_b_gives_none_for_b() {
        let w = win();
        let m = belt_meta(&w, &[0]);
        let (point, a, b) = belt_inputs(&m, "lis2dw ebb36", loader(belt_files(&w))).unwrap();
        assert_eq!(point, 0);
        assert!(a.is_some());
        assert!(b.is_none());
    }
}

fn tiny(n: usize, seed: f64) -> RawSamples {
    RawSamples {
        t: (0..n).map(|i| i as f64 * 0.001).collect(),
        x: (0..n).map(|i| seed + i as f64).collect(),
        y: vec![0.0; n],
        z: vec![0.0; n],
    }
}

/// An axes-map session of the named chips. `files[chip][segment]` is that recording's sample
/// count and coverage, or `None` for no recording.
fn axes_map_meta(chips: Vec<ChipMeta>, files: Vec<[Option<(usize, Coverage)>; 3]>) -> SessionMeta {
    let mut m = meta(chips, Vec::new());
    m.format_version = 2;
    m.axes_map_run = Some(AxesMapRun {
        max_velocity: None,
        speed: 80.0,
        accel: 1000.0,
        z_height: 30.0,
        travel_speed: 150.0,
        max_z_velocity: Some(10.0),
        max_z_accel: Some(100.0),
    });
    m.probe_points = (0..3).map(|_| ProbePoint { position: None }).collect();
    m.segments = (0..3)
        .map(|s| SegmentMeta {
            axis: AXES_MAP_DIRECTIONS[s],
            point: s as u32,
            window: None,
            files: files
                .iter()
                .enumerate()
                .filter_map(|(c, per)| {
                    per[s]
                        .clone()
                        .map(|(count, coverage)| entry(c as u32, s, count, coverage))
                })
                .collect(),
            steps: vec![],
        })
        .collect();
    m
}

fn full(n: usize) -> [Option<(usize, Coverage)>; 3] {
    [
        Some((n, Coverage::Complete)),
        Some((n, Coverage::Complete)),
        Some((n, Coverage::Complete)),
    ]
}

/// Serves `tiny` recordings of the recorded length, except for the files named in `unreadable`.
fn axes_map_loader(
    m: &SessionMeta,
    unreadable: Vec<String>,
) -> impl FnMut(&SegmentFile) -> Result<RawSamples, SessionError> {
    let counts: HashMap<String, usize> = m
        .segments
        .iter()
        .flat_map(|s| &s.files)
        .map(|f| (f.sample_file.clone(), f.sample_count as usize))
        .collect();
    move |f| {
        if unreadable.contains(&f.sample_file) {
            Err(SessionError::BadMagic)
        } else {
            Ok(tiny(counts[&f.sample_file], 1.0))
        }
    }
}

fn missing_of(c: &ChipCapture) -> &str {
    match c {
        ChipCapture::Incomplete { missing, .. } => missing,
        ChipCapture::Ready { .. } => panic!("expected Incomplete"),
    }
}

parity_test! {
    fn the_segment_windows_go_to_the_analysis_input() {
        let mut m = axes_map_meta(vec![chip("adxl345", Health::Ok)], vec![full(6)]);
        m.segments[0].window = Some(TimeWindow { start: 10.0, end: 11.5 });
        m.segments[2].window = Some(TimeWindow { start: 14.0, end: 17.25 });
        let out = axes_map_inputs(&m, axes_map_loader(&m, vec![])).unwrap();
        let ChipCapture::Ready { input, .. } = &out[0] else {
            panic!("expected Ready");
        };
        assert_eq!(input.windows, [Some(10.0..11.5), None, Some(14.0..17.25)]);
    }
}

parity_test! {
    fn three_complete_segments_make_a_ready_input() {
        let mut chips = vec![chip("adxl345", Health::Ok)];
        chips[0].axes_map = Some("x,y,z".to_owned());
        chips[0].mounting = Some("bed".to_owned());
        let m = axes_map_meta(chips, vec![full(6)]);
        let out = axes_map_inputs(&m, axes_map_loader(&m, vec![])).unwrap();
        assert_eq!(out.len(), 1);
        let ChipCapture::Ready { input, health, health_notes } = &out[0] else {
            panic!("expected Ready");
        };
        assert_eq!(input.chip, "adxl345");
        assert_eq!(input.configured_map.as_deref(), Some("x,y,z"));
        assert_eq!(input.mounting, Mounting::Bed);
        assert!(input.segments.iter().all(|s| s.len() == 6));
        assert_eq!(*health, Health::Ok);
        assert!(health_notes.is_empty());

        let params = axes_map_params(&m).unwrap();
        assert_eq!(params.speed, 80.0);
        assert_eq!(params.accel, 1000.0);
        assert_eq!(params.max_z_velocity, Some(10.0));
        assert_eq!(params.max_z_accel, Some(100.0));
    }
}

parity_test! {
    fn a_run_stopped_after_two_segments_is_incomplete_for_every_chip() {
        let two = || [Some((6, Coverage::Complete)), Some((6, Coverage::Complete)), None];
        let m = axes_map_meta(
            vec![chip("a", Health::Ok), chip("b", Health::Ok)],
            vec![two(), two()],
        );
        let out = axes_map_inputs(&m, axes_map_loader(&m, vec![])).unwrap();
        assert_eq!(out.len(), 2);
        for c in &out {
            assert_eq!(missing_of(c), "segment Z has no recording");
        }
    }
}

parity_test! {
    fn one_chip_missing_a_segment_is_incomplete_while_the_other_is_ready() {
        let m = axes_map_meta(
            vec![chip("a", Health::Ok), chip("b", Health::Ok)],
            vec![
                full(6),
                [Some((6, Coverage::Complete)), None, Some((6, Coverage::Complete))],
            ],
        );
        let out = axes_map_inputs(&m, axes_map_loader(&m, vec![])).unwrap();
        assert!(matches!(&out[0], ChipCapture::Ready { input, .. } if input.chip == "a"));
        assert!(matches!(&out[1], ChipCapture::Incomplete { chip, .. } if chip == "b"));
        assert_eq!(missing_of(&out[1]), "segment Y has no recording");
    }
}

parity_test! {
    fn incomplete_coverage_names_what_is_missing() {
        let m = axes_map_meta(
            vec![chip("a", Health::Ok)],
            vec![[
                Some((6, Coverage::Complete)),
                Some((6, Coverage::Incomplete { missing: "the last 2 s".to_owned() })),
                Some((6, Coverage::Complete)),
            ]],
        );
        let out = axes_map_inputs(&m, axes_map_loader(&m, vec![])).unwrap();
        assert_eq!(missing_of(&out[0]), "the last 2 s");
    }
}

parity_test! {
    fn an_unreadable_file_makes_only_its_chip_incomplete() {
        let m = axes_map_meta(
            vec![chip("a", Health::Ok), chip("b", Health::Ok)],
            vec![full(6), full(6)],
        );
        let out = axes_map_inputs(&m, axes_map_loader(&m, vec!["c1-s1.zst".to_owned()])).unwrap();
        assert!(matches!(&out[0], ChipCapture::Ready { .. }));
        let missing = missing_of(&out[1]);
        assert!(missing.contains("segment Y") && missing.contains("magic"), "{missing}");

        // A file with the wrong sample count is treated alike.
        let mut wrong = m.clone();
        wrong.segments[2].files[0].sample_count = 7;
        let out = axes_map_inputs(&wrong, axes_map_loader(&m, vec![])).unwrap();
        let missing = missing_of(&out[0]);
        assert!(missing.contains("segment Z") && missing.contains("7"), "{missing}");
        assert!(matches!(&out[1], ChipCapture::Ready { .. }));
    }
}

parity_test! {
    fn health_is_the_worst_of_chip_and_files_with_joined_notes() {
        let mut chips = vec![chip("a", Health::Ok)];
        chips[0].health_notes = vec!["chip note".to_owned(), "shared".to_owned()];
        let mut m = axes_map_meta(chips, vec![full(6)]);
        m.segments[0].files[0].health_notes = vec!["shared".to_owned(), "x note".to_owned()];
        m.segments[1].files[0].health = Some(Health::Degraded);
        m.segments[1].files[0].health_notes = vec!["y note".to_owned()];
        let out = axes_map_inputs(&m, axes_map_loader(&m, vec![])).unwrap();
        let ChipCapture::Ready { health, health_notes, .. } = &out[0] else {
            panic!("expected Ready");
        };
        assert_eq!(*health, Health::Degraded);
        assert_eq!(health_notes, &["chip note", "shared", "x note", "y note"]);
    }
}

parity_test! {
    fn the_chip_result_serialises_with_its_state() {
        let failed = AxesMapChip::Failed { chip: "a".to_owned(), message: "boom".to_owned() };
        let v = serde_json::to_value(&failed).unwrap();
        assert_eq!(v["state"], "Failed");
        assert_eq!(v["message"], "boom");
    }
}

#[test]
fn axes_map_params_use_recorded_velocity_limit_and_keep_legacy_speed() {
    let mut m = axes_map_meta(vec![chip("a", Health::Ok)], vec![full(6)]);
    assert_eq!(axes_map_params(&m).unwrap().speed, 80.0);
    m.axes_map_run.as_mut().unwrap().max_velocity = Some(10.0);
    assert_eq!(axes_map_params(&m).unwrap().speed, 10.0);
    m.axes_map_run.as_mut().unwrap().max_velocity = Some(0.0);
    assert!(axes_map_params(&m).is_err());
}

mod vibrations {
    use std::collections::HashMap;

    use wobblescope_core::RawSamples;
    use wobblescope_core::direction::Direction;
    use wobblescope_core::eligibility::{Coverage, Decision, Health};
    use wobblescope_core::vibrations::{Grid, Kinematics, VibrationsParams, worst_case};
    use wobblescope_session::SessionError;
    use wobblescope_session::bridge::{analyze_vibrations, vibrations_inputs};
    use wobblescope_session::meta::{
        ChipMeta, Outcome, ProbePoint, SegmentFile, SegmentMeta, SessionKind, SessionMeta, Source,
        StepMeta, TimeDomain, TimeWindow, VibrationsRun, sample_file_name,
    };

    const RATE: f64 = 1600.0;
    const SPEEDS: [f64; 4] = [20.0, 30.0, 40.0, 50.0];

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

    /// One segment of four one-second steps, each followed by a one-second gap, sampled at a
    /// steady rate. A step is a tone as loud as `amplitude` times its speed. The gap is loud
    /// noise that must never reach an analysis.
    fn recording(amplitude: f64) -> RawSamples {
        let mut s = RawSamples::default();
        let mut seed = 12345_u64;
        for i in 0..(SPEEDS.len() * 2 * RATE as usize) {
            let t = i as f64 / RATE;
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let noise = ((seed >> 33) as f64 / f64::from(1_u32 << 31)) - 0.5;
            let step = (i / RATE as usize) / 2;
            let in_step = (i / RATE as usize).is_multiple_of(2);
            let level = if in_step {
                amplitude * SPEEDS[step]
            } else {
                1000.0
            };
            s.t.push(t);
            s.x.push(level * (2.0 * std::f64::consts::PI * 80.0 * t).sin() + noise);
            s.y.push(level * 0.5 * (2.0 * std::f64::consts::PI * 110.0 * t).sin() + noise);
            s.z.push(noise);
        }
        s
    }

    fn segment(axis: Direction, files: Vec<SegmentFile>) -> SegmentMeta {
        let total = (SPEEDS.len() * 2) as f64;
        SegmentMeta {
            axis,
            point: 0,
            window: Some(TimeWindow {
                start: 0.0,
                end: total,
            }),
            files,
            steps: SPEEDS
                .iter()
                .enumerate()
                .map(|(k, &speed)| StepMeta {
                    speed,
                    window: TimeWindow {
                        start: 2.0 * k as f64,
                        end: 2.0 * k as f64 + 1.0,
                    },
                })
                .collect(),
        }
    }

    fn file(chip: u32, segment: usize) -> SegmentFile {
        SegmentFile {
            chip,
            sample_file: sample_file_name(chip as usize, segment),
            sample_count: (SPEEDS.len() * 2 * RATE as usize) as u32,
            coverage: Coverage::Complete,
            health: None,
            health_notes: Vec::new(),
        }
    }

    /// A Cartesian vibrations session with one chip per `(name, health, amplitude)`, and the
    /// sample files of all of them by file name.
    fn session(chips: &[(&str, Health, f64)]) -> (SessionMeta, HashMap<String, RawSamples>) {
        let mut files = HashMap::new();
        let mut per_segment: [Vec<SegmentFile>; 2] = [vec![], vec![]];
        for (index, (_, _, amplitude)) in chips.iter().enumerate() {
            for (segment, list) in per_segment.iter_mut().enumerate() {
                let entry = file(index as u32, segment);
                files.insert(entry.sample_file.clone(), recording(*amplitude));
                list.push(entry);
            }
        }
        let [x, y] = per_segment;
        let meta = SessionMeta {
            format_version: 1,
            id: "vibrations".to_owned(),
            created_at: "2026-11-16T10:00:00Z".to_owned(),
            source: Source::Capture,
            time_domain: TimeDomain::KlipperPrintTime,
            mode: None,
            settings: None,
            printer: None,
            outcome: Outcome::Complete,
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
                angular_step: None,
            }),
            notes: vec![],
            probe_points: vec![ProbePoint { position: None }],
            chips: chips.iter().map(|(n, h, _)| chip(n, *h)).collect(),
            segments: vec![segment(Direction::X, x), segment(Direction::Y, y)],
        };
        (meta, files)
    }

    fn loader<'a>(
        files: &'a HashMap<String, RawSamples>,
    ) -> impl FnMut(&SegmentFile) -> Result<RawSamples, SessionError> + 'a {
        |file| {
            files
                .get(&file.sample_file)
                .cloned()
                .ok_or_else(|| SessionError::Invalid {
                    field: file.sample_file.clone(),
                    reason: "sample file is not loaded".to_owned(),
                })
        }
    }

    fn grid_of(result: &wobblescope_core::vibrations::VibrationsResult) -> Grid {
        Grid {
            angles: result.angles.clone(),
            speeds: result.speeds.clone(),
            data: result.grid.clone(),
        }
    }

    parity_test! {
        fn steps_are_cut_from_the_segment_file_by_their_windows() {
            let (meta, files) = session(&[("a", Health::Ok, 1.0)]);
            let steps = vibrations_inputs(&meta, "a", loader(&files)).unwrap();
            assert_eq!(steps.len(), 2 * SPEEDS.len());
            for (i, step) in steps.iter().enumerate() {
                let (angle, k) = if i < SPEEDS.len() { (0.0, i) } else { (90.0, i - SPEEDS.len()) };
                assert_eq!(step.angle, angle);
                assert_eq!(step.speed, SPEEDS[k]);
                assert_eq!(step.samples.len(), RATE as usize);
                assert_eq!(step.samples.t[0], 2.0 * k as f64);
                assert!(step.samples.t.iter().all(|t| (2.0 * k as f64..2.0 * k as f64 + 1.0).contains(t)));
            }
        }
    }

    parity_test! {
        fn a_recorded_run_analyses_end_to_end() {
            let (meta, files) = session(&[("a", Health::Ok, 1.0), ("b", Health::Ok, 2.0)]);
            let out = analyze_vibrations(&meta, 200.0, false, loader(&files)).unwrap();
            assert_eq!(out.per_chip.len(), 2);
            for chip in &out.per_chip {
                assert!(chip.excluded.is_none() && chip.failure.is_none(), "{chip:?}");
                assert!(chip.result.is_some(), "{chip:?}");
            }
            let worst = out.worst_case.expect("a worst case over both chips");
            // The grid has six points per measured speed, from the lowest to the highest.
            let points = 6 * SPEEDS.len();
            assert_eq!(worst.speeds.len(), points);
            assert_eq!((worst.speeds[0], worst.speeds[points - 1]), (20.0, 50.0));
            assert_eq!(worst.angles.len(), worst.angle_energy.len());
            assert_eq!(worst.grid.len(), worst.angles.len());
            assert!(worst.grid.iter().all(|row| row.len() == points));
            for curve in [&worst.speed_min, &worst.speed_max, &worst.speed_variance, &worst.vibration_metric] {
                assert_eq!(curve.len(), points);
            }
            for chip in &out.per_chip {
                let result = chip.result.as_ref().unwrap();
                assert_eq!(result.steps.len(), 2 * SPEEDS.len());
                // The motor profile belongs to a chip's own result. The worst case has none.
                assert!(result.motor.is_some(), "{chip:?}");
            }
            assert!(worst.motor.is_none());
        }
    }

    parity_test! {
        fn a_chip_missing_one_angle_is_excluded_with_a_reason() {
            let (mut meta, files) = session(&[("a", Health::Ok, 1.0), ("b", Health::Ok, 1.0)]);
            meta.segments[1].files.retain(|f| f.chip != 1);
            let out = analyze_vibrations(&meta, 200.0, false, loader(&files)).unwrap();
            let b = &out.per_chip[1];
            assert_eq!(b.chip, "b");
            assert_eq!(b.decisions.len(), 1);
            assert!(b.excluded.as_deref().unwrap().contains("90"));
            assert!(b.result.is_none() && b.failure.is_none());
            let a = &out.per_chip[0];
            assert!(a.excluded.is_none());
            assert_eq!(out.worst_case.as_ref().unwrap().grid, a.result.as_ref().unwrap().grid);
        }
    }

    parity_test! {
        fn a_degraded_chip_is_left_out_of_the_worst_case_but_keeps_its_own_result() {
            let (meta, files) = session(&[("a", Health::Ok, 1.0), ("b", Health::Degraded, 5.0)]);
            let out = analyze_vibrations(&meta, 200.0, false, loader(&files)).unwrap();
            let b = &out.per_chip[1];
            assert!(b.excluded.as_deref().unwrap().contains("degraded"));
            assert!(b.decisions.iter().all(|d| d.decision == Decision::Excluded));
            assert!(b.result.is_some() && b.failure.is_none());
            let worst = out.worst_case.as_ref().unwrap();
            assert!(worst.steps.is_empty() && worst.motor.is_none());
            assert_eq!(worst.grid, out.per_chip[0].result.as_ref().unwrap().grid);

            let forced = analyze_vibrations(&meta, 200.0, true, loader(&files)).unwrap();
            assert!(forced.per_chip[1].excluded.is_none());
            assert!(forced.per_chip[1].decisions.iter().all(|d| d.decision == Decision::Overridden));
            assert_ne!(forced.worst_case.unwrap().grid, worst.grid);
        }
    }

    parity_test! {
        fn an_incomplete_session_is_refused() {
            let (mut meta, files) = session(&[("a", Health::Ok, 1.0)]);
            meta.outcome = Outcome::Incomplete { reason: "stopped".to_owned() };
            let err = analyze_vibrations(&meta, 200.0, false, loader(&files)).unwrap_err();
            assert!(err.to_string().contains("incomplete"), "{err}");

            let (mut meta, files) = session(&[("a", Health::Ok, 1.0)]);
            meta.vibrations_run = None;
            assert!(analyze_vibrations(&meta, 200.0, false, loader(&files)).is_err());

            let (mut meta, files) = session(&[("a", Health::Ok, 1.0)]);
            meta.vibrations_run.as_mut().unwrap().kinematics = "delta".to_owned();
            assert!(analyze_vibrations(&meta, 200.0, false, loader(&files)).is_err());
        }
    }

    parity_test! {
        fn worst_case_uses_only_eligible_chips() {
            let (meta, files) = session(&[
                ("a", Health::Ok, 1.0),
                ("b", Health::Ok, 3.0),
                ("c", Health::Degraded, 50.0),
            ]);
            let out = analyze_vibrations(&meta, 200.0, false, loader(&files)).unwrap();
            let grid = |i: usize| grid_of(out.per_chip[i].result.as_ref().unwrap());
            let measured = |i: usize| out.per_chip[i].result.as_ref().unwrap().measured.clone();
            let params = VibrationsParams { max_freq: 200.0, kinematics: Kinematics::Cartesian };
            let expected = worst_case(&[(&grid(0), &measured(0)), (&grid(1), &measured(1))], &params).unwrap();
            assert_eq!(out.worst_case.as_ref().unwrap(), &expected);
            let with_c = worst_case(
                &[(&grid(0), &measured(0)), (&grid(1), &measured(1)), (&grid(2), &measured(2))],
                &params,
            ).unwrap();
            assert_ne!(expected.grid, with_c.grid);
        }
    }

    parity_test! {
        fn a_missing_file_of_one_chip_does_not_stop_another_chip_or_the_worst_case() {
            let (meta, mut files) = session(&[("a", Health::Ok, 1.0), ("b", Health::Ok, 2.0)]);
            files.remove(&sample_file_name(1, 1));
            let out = analyze_vibrations(&meta, 200.0, false, loader(&files)).unwrap();
            let b = &out.per_chip[1];
            assert!(b.result.is_none());
            assert!(b.failure.as_deref().unwrap().contains(&sample_file_name(1, 1)));
            let a = out.per_chip[0].result.as_ref().unwrap();
            assert!(out.per_chip[0].failure.is_none());
            assert_eq!(out.worst_case.as_ref().unwrap().grid, a.grid);
        }
    }
    parity_test! {
        fn a_corexy_session_maps_its_diagonal_segments_to_45_and_135_degrees() {
            let (mut meta, files) = session(&[("a", Health::Ok, 1.0), ("b", Health::Ok, 1.0)]);
            meta.vibrations_run.as_mut().unwrap().kinematics = "corexy".to_owned();
            meta.format_version = 2;
            // The daemon writes the 45 degree segment as BELT_B and the 135 degree one as BELT_A.
            meta.segments[0].axis = Direction::Vector([1, 1, 0]);
            meta.segments[1].axis = Direction::Vector([1, -1, 0]);
            assert_eq!(meta.kind(), SessionKind::Vibrations);

            let steps = vibrations_inputs(&meta, "a", loader(&files)).unwrap();
            assert_eq!(steps.len(), 2 * SPEEDS.len());
            for (i, step) in steps.iter().enumerate() {
                let angle = if i < SPEEDS.len() { 45.0 } else { 135.0 };
                assert_eq!(step.angle, angle);
            }

            let out = analyze_vibrations(&meta, 200.0, false, loader(&files)).unwrap();
            for chip in &out.per_chip {
                assert!(chip.excluded.is_none() && chip.failure.is_none(), "{chip:?}");
                assert!(chip.result.is_some(), "{chip:?}");
            }
            assert!(out.worst_case.is_some());

            meta.segments[1].files.retain(|f| f.chip != 1);
            let out = analyze_vibrations(&meta, 200.0, false, loader(&files)).unwrap();
            let b = &out.per_chip[1];
            assert!(b.excluded.as_deref().unwrap().contains("no recording at 135 degrees"));
            assert!(b.result.is_none() && b.failure.is_none());
            assert!(out.per_chip[0].excluded.is_none());
            assert!(out.worst_case.is_some());
        }
    }

    parity_test! {
        fn a_chip_whose_analysis_fails_is_marked_as_left_out_of_the_worst_case() {
            let (meta, mut files) = session(&[("a", Health::Ok, 1.0), ("b", Health::Ok, 2.0)]);
            files.remove(&sample_file_name(1, 1));
            let out = analyze_vibrations(&meta, 200.0, false, loader(&files)).unwrap();
            let b = &out.per_chip[1];
            let failure = b.failure.as_deref().unwrap();
            let excluded = b.excluded.as_deref().unwrap();
            assert!(excluded.contains("left out of All chips"), "{excluded}");
            assert!(excluded.contains(failure), "{excluded}");
            assert!(out.per_chip[0].excluded.is_none());
        }
    }
}
