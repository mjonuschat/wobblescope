#[macro_use]
mod common;

use wobblescope_core::direction::Direction;
use wobblescope_core::eligibility::{Coverage, Health};
use wobblescope_core::fit::Axis;
use wobblescope_session::SessionError;
use wobblescope_session::codec::MAX_SAMPLES;
use wobblescope_session::meta::{
    AXES_MAP_DIRECTIONS, AxesMapRun, ChipMeta, Mode, Outcome, PrinterInfo, ProbePoint, SegmentFile,
    SegmentMeta, SessionKind, SessionMeta, Source, StepMeta, SweepSettings, TimeDomain, TimeWindow,
    VibrationsRun, sample_file_name, validate_session_id,
};

fn chip(name: &str, chip_type: Option<&str>, rate: Option<f64>) -> ChipMeta {
    ChipMeta {
        name: name.to_owned(),
        chip_type: chip_type.map(str::to_owned),
        nominal_rate_hz: rate,
        observed_rate_hz: rate,
        axes_map: None,
        mounting: None,
        health: Health::Ok,
        health_notes: vec![],
    }
}

fn file(chip: u32, segment: usize, count: u32) -> SegmentFile {
    SegmentFile {
        chip,
        sample_file: sample_file_name(chip as usize, segment),
        sample_count: count,
        coverage: Coverage::Complete,
        health: None,
        health_notes: Vec::new(),
    }
}

/// Two chips, X and Y segments at one probe point, full capture metadata.
fn capture_meta() -> SessionMeta {
    SessionMeta {
        format_version: 1,
        id: "ws_x-1".to_owned(),
        created_at: "2026-10-31T19:00:00Z".to_owned(),
        source: Source::Capture,
        time_domain: TimeDomain::KlipperPrintTime,
        mode: Some(Mode::Sweep),
        settings: Some(SweepSettings {
            freq_start: 5.0,
            freq_end: 135.0,
            hz_per_sec: 1.0,
            accel_per_hz: 60.0,
            sweeping_accel: Some(400.0),
            sweeping_period: Some(1.2),
        }),
        printer: Some(PrinterInfo {
            name: Some("voron".to_owned()),
            klipper_version: Some("v0.13.0-658-gbd09e017".to_owned()),
            mcu_firmware: vec![],
            square_corner_velocity: Some(5.0),
            damping_ratio_x: Some(0.12),
            damping_ratio_y: None,
        }),
        outcome: Outcome::Complete,
        axes_map_run: None,
        vibrations_run: None,
        notes: vec![],
        probe_points: vec![ProbePoint {
            position: Some([150.0, 150.0, 50.0]),
        }],
        chips: vec![
            chip("adxl345", Some("ADXL345"), Some(3200.0)),
            chip("lis2dw ebb36", Some("LIS2DW"), Some(400.0)),
        ],
        segments: vec![
            SegmentMeta {
                axis: Axis::X.into(),
                point: 0,
                window: Some(TimeWindow {
                    start: 41230.322878,
                    end: 41262.104211,
                }),
                files: vec![file(0, 0, 100_000), file(1, 0, 12_000)],
                steps: vec![],
            },
            SegmentMeta {
                axis: Axis::Y.into(),
                point: 0,
                window: None,
                files: vec![file(0, 1, 100_000), file(1, 1, 12_000)],
                steps: vec![],
            },
        ],
    }
}

fn invalid_field(err: SessionError) -> String {
    match err {
        SessionError::Invalid { field, .. } => field,
        other => panic!("expected Invalid, got {other:?}"),
    }
}

parity_test! {
    fn capture_shaped_meta_round_trips_through_json_exactly() {
        let meta = capture_meta();
        let json = meta.to_json().unwrap();
        let back = SessionMeta::from_json(&json).unwrap();
        assert_eq!(back, meta);
        let w = back.segments[0].window.unwrap();
        assert_eq!(w.start.to_bits(), 41230.322878f64.to_bits());
        assert_eq!(w.end.to_bits(), 41262.104211f64.to_bits());
    }
}

parity_test! {
    fn find_file_looks_up_a_recording_by_chip_and_segment() {
        let mut meta = capture_meta();
        meta.segments[1].files.remove(1);
        assert_eq!(meta.find_file(0, 0), Some(&file(0, 0, 100_000)));
        assert_eq!(meta.find_file(1, 0), Some(&file(1, 0, 12_000)));
        assert_eq!(meta.find_file(0, 1), Some(&file(0, 1, 100_000)));
        assert_eq!(meta.find_file(1, 1), None);
        assert_eq!(meta.find_file(2, 0), None);
        assert_eq!(meta.find_file(0, 5), None);
    }
}

parity_test! {
    fn import_shaped_meta_parses_and_validates() {
        let json = r#"{
            "format_version": 1,
            "id": "import-1",
            "created_at": "2026-10-31T19:00:00Z",
            "source": {"CsvImport": {"original_file_name": "raw_data_x_ws.csv"}},
            "time_domain": "KlipperPrintTime",
            "outcome": "Complete",
            "probe_points": [{}],
            "chips": [{"name": "adxl345", "health": "Unknown"}],
            "segments": [{"axis": "X", "point": 0, "files": [
                {"chip": 0, "sample_file": "c0-s0.zst", "sample_count": 1000, "coverage": "Complete"}
            ]}]
        }"#;
        let meta = SessionMeta::from_json(json).unwrap();
        assert!(meta.mode.is_none() && meta.settings.is_none() && meta.printer.is_none());
        assert!(meta.chips[0].chip_type.is_none());
        assert_eq!(meta.chips[0].health, Health::Ok);
    }
}

parity_test! {
    fn a_newer_format_version_is_reported_even_when_the_shape_changed() {
        let err = SessionMeta::from_json(r#"{"format_version": 4, "renamed": true}"#).unwrap_err();
        assert!(matches!(err, SessionError::UnsupportedFormatVersion { found: 4 }));
    }
}

parity_test! {
    fn unknown_fields_are_ignored() {
        let mut v: serde_json::Value = serde_json::from_str(&capture_meta().to_json().unwrap()).unwrap();
        v["future_field"] = serde_json::json!({"a": 1});
        v["chips"][0]["future_chip_field"] = serde_json::json!(true);
        let back = SessionMeta::from_json(&v.to_string()).unwrap();
        assert_eq!(back, capture_meta());
    }
}

parity_test! {
    fn a_meta_without_format_version_is_an_error() {
        let mut v: serde_json::Value = serde_json::from_str(&capture_meta().to_json().unwrap()).unwrap();
        v.as_object_mut().unwrap().remove("format_version");
        assert!(SessionMeta::from_json(&v.to_string()).is_err());
    }
}

parity_test! {
    fn unsafe_or_non_canonical_sample_file_names_fail_validation() {
        for bad in [
            "../meta.json", "meta.json", "a/b", "a\\b", "..", ".hidden", "/etc/passwd", "",
            "c1-s0.zst",
        ] {
            let mut m = capture_meta();
            m.segments[0].files[0].sample_file = bad.to_owned();
            let field = invalid_field(m.validate().unwrap_err());
            assert_eq!(field, "segments[0].files[0].sample_file", "{bad:?}");
        }
    }
}

parity_test! {
    fn structural_violations_fail_validation() {
        let mut dup_chip = capture_meta();
        dup_chip.chips[1].name = "adxl345".to_owned();
        assert_eq!(invalid_field(dup_chip.validate().unwrap_err()), "chips[1].name");

        let mut chip_range = capture_meta();
        chip_range.chips.pop();
        assert_eq!(
            invalid_field(chip_range.validate().unwrap_err()),
            "segments[0].files[1].chip"
        );

        let mut point_range = capture_meta();
        point_range.segments[1].point = 1;
        assert_eq!(invalid_field(point_range.validate().unwrap_err()), "segments[1].point");

        let mut dup_file_chip = capture_meta();
        dup_file_chip.segments[0].files[1].chip = 0;
        dup_file_chip.segments[0].files[1].sample_file = sample_file_name(0, 0);
        assert_eq!(
            invalid_field(dup_file_chip.validate().unwrap_err()),
            "segments[0].files[1].chip"
        );

        let mut dup_segment = capture_meta();
        dup_segment.segments[1].axis = Axis::X.into();
        assert_eq!(invalid_field(dup_segment.validate().unwrap_err()), "segments[1]");

        let mut dup_name = capture_meta();
        dup_name.segments[1].files[0].sample_file = dup_name.segments[0].files[0].sample_file.clone();
        assert!(dup_name.validate().is_err());
    }
}

parity_test! {
    fn bad_windows_fail_validation() {
        for (start, end) in [
            (f64::NAN, 1.0),
            (5.0, 5.0),
            (6.0, 5.0),
            (1.0, f64::INFINITY),
        ] {
            let mut m = capture_meta();
            m.segments[0].window = Some(TimeWindow { start, end });
            assert_eq!(
                invalid_field(m.validate().unwrap_err()),
                "segments[0].window",
                "{start}..{end}"
            );
        }
    }
}

parity_test! {
    fn non_positive_or_non_finite_rates_fail_validation() {
        for rate in [0.0, -1.0, f64::NAN] {
            let mut m = capture_meta();
            m.chips[0].nominal_rate_hz = Some(rate);
            assert_eq!(invalid_field(m.validate().unwrap_err()), "chips[0].nominal_rate_hz");
            let mut m = capture_meta();
            m.chips[1].observed_rate_hz = Some(rate);
            assert_eq!(invalid_field(m.validate().unwrap_err()), "chips[1].observed_rate_hz");
        }
    }
}

parity_test! {
    fn a_sample_count_over_the_codec_limit_fails_validation() {
        let mut m = capture_meta();
        m.segments[0].files[0].sample_count = MAX_SAMPLES as u32 + 1;
        assert_eq!(
            invalid_field(m.validate().unwrap_err()),
            "segments[0].files[0].sample_count"
        );
    }
}

parity_test! {
    fn session_ids_follow_the_klipper_name_rule() {
        for bad in ["a b", "../x", "", &"a".repeat(65)] {
            assert!(validate_session_id(bad).is_err(), "{bad:?}");
        }
        assert!(validate_session_id("ws_x-1").is_ok());
        assert!(validate_session_id(&"a".repeat(64)).is_ok());
        let mut m = capture_meta();
        m.id = "a b".to_owned();
        assert_eq!(invalid_field(m.validate().unwrap_err()), "id");
    }
}

parity_test! {
    fn the_axis_serializes_as_its_name() {
        assert_eq!(serde_json::to_string(&Axis::X).unwrap(), "\"X\"");
    }
}

#[test]
fn per_segment_health_round_trips_and_older_metadata_without_it_still_loads() {
    use wobblescope_core::eligibility::Health;

    let mut m = capture_meta();
    m.segments[0].files[0].health = Some(Health::Degraded);
    m.segments[0].files[0].health_notes = vec!["the chip's errors counter rose by 3".to_owned()];
    let back = SessionMeta::from_json(&m.to_json().unwrap()).unwrap();
    assert_eq!(back, m);

    // A session written before the field existed has neither key.
    let mut value: serde_json::Value = serde_json::from_str(&m.to_json().unwrap()).unwrap();
    for segment in value["segments"].as_array_mut().unwrap() {
        for file in segment["files"].as_array_mut().unwrap() {
            let file = file.as_object_mut().unwrap();
            file.remove("health");
            file.remove("health_notes");
        }
    }
    let old = SessionMeta::from_json(&value.to_string()).unwrap();
    assert_eq!(old.segments[0].files[0].health, None);
    assert!(old.segments[0].files[0].health_notes.is_empty());
}

parity_test! {
    fn an_old_csv_import_without_file_names_still_parses() {
        let source: Source = serde_json::from_str(r#"{"CsvImport": {"original_file_name": "a.csv"}}"#).unwrap();
        let Source::CsvImport { original_file_name, original_file_names } = source else {
            panic!("expected CsvImport");
        };
        assert_eq!(original_file_name.as_deref(), Some("a.csv"));
        assert!(original_file_names.is_empty());
    }
}

/// Segments of the given directions, each at the given point (two probe points exist).
fn vector_meta(segments: &[(Direction, u32)]) -> SessionMeta {
    let mut meta = capture_meta();
    meta.format_version = 2;
    meta.probe_points.push(ProbePoint { position: None });
    meta.segments = segments
        .iter()
        .enumerate()
        .map(|(i, &(axis, point))| SegmentMeta {
            axis,
            point,
            window: None,
            files: vec![file(0, i, 100)],
            steps: vec![],
        })
        .collect();
    meta
}

fn belts_at(a: u32, b: u32) -> SessionMeta {
    vector_meta(&[(Direction::BELT_A, a), (Direction::BELT_B, b)])
}

parity_test! {
    fn a_version_two_session_with_vector_segments_round_trips() {
        let meta = belts_at(0, 0);
        let json = meta.to_json().unwrap();
        assert!(json.contains("\"Vector\""));
        assert_eq!(SessionMeta::from_json(&json).unwrap(), meta);
    }
}

parity_test! {
    fn a_vector_segment_in_a_version_one_session_is_refused() {
        let mut meta = belts_at(0, 0);
        meta.format_version = 1;
        assert_eq!(invalid_field(meta.validate().unwrap_err()), "format_version");
    }
}

parity_test! {
    fn a_version_four_session_is_refused_with_its_version() {
        let mut meta = belts_at(0, 0);
        meta.format_version = 4;
        assert!(matches!(
            meta.validate().unwrap_err(),
            SessionError::UnsupportedFormatVersion { found: 4 }
        ));
    }
}

parity_test! {
    fn x_y_and_vector_segments_cannot_mix() {
        let mut meta = vector_meta(&[(Direction::X, 0), (Direction::BELT_A, 0)]);
        assert_eq!(invalid_field(meta.validate().unwrap_err()), "segments");
        meta.segments.reverse();
        assert_eq!(invalid_field(meta.validate().unwrap_err()), "segments");
    }
}

parity_test! {
    fn a_zero_vector_is_refused() {
        let meta = vector_meta(&[(Direction::Vector([0, 0, 0]), 0)]);
        assert_eq!(invalid_field(meta.validate().unwrap_err()), "segments[0].axis");
    }
}

parity_test! {
    fn a_point_holds_each_vector_once() {
        let meta = vector_meta(&[(Direction::BELT_A, 0), (Direction::BELT_A, 0)]);
        assert_eq!(invalid_field(meta.validate().unwrap_err()), "segments[1]");
        vector_meta(&[(Direction::BELT_A, 0), (Direction::BELT_A, 1)])
            .validate()
            .unwrap();
    }
}

parity_test! {
    fn required_version_is_two_only_with_a_vector_segment() {
        assert_eq!(capture_meta().required_version(), 1);
        let mut empty = capture_meta();
        empty.segments.clear();
        assert_eq!(empty.required_version(), 1);
        assert_eq!(belts_at(0, 0).required_version(), 2);
    }
}

parity_test! {
    fn kind_is_axes_for_x_y_and_empty_sessions() {
        assert_eq!(capture_meta().kind(), SessionKind::Axes);
        let mut empty = capture_meta();
        empty.segments.clear();
        assert_eq!(empty.kind(), SessionKind::Axes);
    }
}

parity_test! {
    fn kind_is_belts_for_one_or_both_diagonals_at_one_point() {
        assert_eq!(belts_at(0, 0).kind(), SessionKind::Belts);
        assert_eq!(vector_meta(&[(Direction::BELT_B, 1)]).kind(), SessionKind::Belts);
        assert_eq!(vector_meta(&[(Direction::BELT_A, 0)]).kind(), SessionKind::Belts);
    }
}

parity_test! {
    fn kind_is_other_for_another_vector_or_the_belts_at_two_points() {
        assert_eq!(belts_at(0, 1).kind(), SessionKind::Other);
        assert_eq!(
            vector_meta(&[(Direction::Vector([1, 0, 1]), 0)]).kind(),
            SessionKind::Other
        );
        assert_eq!(
            vector_meta(&[(Direction::BELT_A, 0), (Direction::Vector([1, 0, 1]), 0)]).kind(),
            SessionKind::Other
        );
    }
}

/// A complete axes-map session: one chip file in each of the X, Y and Z moves.
fn axes_map_meta() -> SessionMeta {
    let mut m = capture_meta();
    m.format_version = 2;
    m.axes_map_run = Some(AxesMapRun {
        max_velocity: None,
        speed: 80.0,
        accel: 1000.0,
        z_height: 30.0,
        travel_speed: 150.0,
        max_z_velocity: Some(10.0),
        max_z_accel: None,
    });
    m.mode = None;
    m.settings = None;
    m.probe_points = (0..3)
        .map(|i| ProbePoint {
            position: Some([100.0 + f64::from(i), 100.0, 30.0]),
        })
        .collect();
    m.segments = AXES_MAP_DIRECTIONS
        .iter()
        .enumerate()
        .map(|(i, &axis)| SegmentMeta {
            axis,
            point: i as u32,
            window: None,
            files: vec![file(0, i, 10)],
            steps: vec![],
        })
        .collect();
    m
}

parity_test! {
    fn a_session_without_axes_map_run_reads_as_before() {
        let m = capture_meta();
        let json = m.to_json().unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value.as_object_mut().unwrap().remove("axes_map_run");
        assert_eq!(SessionMeta::from_json(&value.to_string()).unwrap(), m);
    }
}

parity_test! {
    fn legacy_unit_vector_sessions_remain_valid_without_axes_map_run() {
        for len in 1..=3 {
            let mut m = axes_map_meta();
            m.axes_map_run = None;
            m.segments.truncate(len);
            m.mode = Some(Mode::Pulse);
            m.segments.iter_mut().for_each(|s| s.point = 0);
            assert_eq!(m.kind(), SessionKind::Other);
            m.validate().unwrap();
            let mut value = serde_json::to_value(&m).unwrap();
            value.as_object_mut().unwrap().remove("axes_map_run");
            assert_eq!(SessionMeta::from_json(&value.to_string()).unwrap(), m);
        }
    }
}

parity_test! {
    fn axes_map_run_rejects_empty_and_malformed_metadata() {
        let mut m = axes_map_meta();
        m.segments.clear();
        m.outcome = Outcome::Failed { reason: "stopped".to_owned() };
        assert_eq!(m.kind(), SessionKind::AxesMap);
        assert_eq!(invalid_field(m.validate().unwrap_err()), "axes_map_run");
        let mut value = serde_json::to_value(axes_map_meta()).unwrap();
        value["axes_map_run"] = serde_json::json!({"speed": 80});
        assert!(SessionMeta::from_json(&value.to_string()).is_err());
    }
}

parity_test! {
    fn an_axes_map_session_round_trips_as_version_2() {
        let m = axes_map_meta();
        assert_eq!(m.kind(), SessionKind::AxesMap);
        assert_eq!(m.required_version(), 2);
        let json = m.to_json().unwrap();
        assert!(json.contains("\"format_version\": 2"));
        assert_eq!(SessionMeta::from_json(&json).unwrap(), m);

        let mut wrong = m.clone();
        wrong.format_version = 1;
        assert_eq!(invalid_field(wrong.validate().unwrap_err()), "format_version");
    }
}

parity_test! {
    fn axes_map_run_identifies_calibrations_even_with_invalid_directions() {
        for len in 1..=3 {
            let mut m = axes_map_meta();
            m.segments.truncate(len);
            assert_eq!(m.kind(), SessionKind::AxesMap, "{len} segments");
        }
        let mut swapped = axes_map_meta();
        swapped.segments.swap(0, 1);
        assert_eq!(swapped.kind(), SessionKind::AxesMap);
        assert_eq!(invalid_field(swapped.validate().unwrap_err()), "axes_map_run");
        let mut skipped = axes_map_meta();
        skipped.segments.remove(0);
        assert_eq!(skipped.kind(), SessionKind::AxesMap);
        assert_eq!(invalid_field(skipped.validate().unwrap_err()), "axes_map_run");
        assert_eq!(serde_json::to_string(&SessionKind::AxesMap).unwrap(), "\"axesmap\"");
    }
}

parity_test! {
    fn axes_map_run_requires_valid_calibration_segments() {
        let mut m = axes_map_meta();
        m.axes_map_run = None;
        assert_eq!(m.kind(), SessionKind::Other);
        m.validate().unwrap();

        let mut r = capture_meta();
        r.axes_map_run = axes_map_meta().axes_map_run;
        assert_eq!(invalid_field(r.validate().unwrap_err()), "axes_map_run");

        let mut four = axes_map_meta();
        four.probe_points.push(ProbePoint { position: None });
        four.segments.push(SegmentMeta {
            axis: AXES_MAP_DIRECTIONS[0],
            point: 3,
            window: None,
            files: vec![],
 steps: vec![],
 });
        assert_eq!(four.kind(), SessionKind::AxesMap);
        assert_eq!(invalid_field(four.validate().unwrap_err()), "axes_map_run");
    }
}

parity_test! {
    fn an_axes_map_session_keeps_each_segment_at_its_own_point() {
        let mut swapped = axes_map_meta();
        swapped.segments[1].point = 2;
        swapped.segments[2].point = 1;
        assert_eq!(invalid_field(swapped.validate().unwrap_err()), "segments[1].point");
    }
}

parity_test! {
    fn an_axes_map_session_has_no_test_mode_or_sweep_settings() {
        let mut m = axes_map_meta();
        m.mode = Some(Mode::Pulse);
        assert_eq!(invalid_field(m.validate().unwrap_err()), "mode");
        let mut m = axes_map_meta();
        m.settings = capture_meta().settings;
        assert_eq!(invalid_field(m.validate().unwrap_err()), "settings");
    }
}

parity_test! {
    fn a_complete_axes_map_session_has_three_probe_points() {
        let mut m = axes_map_meta();
        m.segments.truncate(2);
        m.probe_points.truncate(2);
        assert_eq!(invalid_field(m.validate().unwrap_err()), "probe_points");
        m.outcome = Outcome::Failed {
            reason: "stopped".to_owned(),
        };
        m.validate().unwrap();
    }
}

parity_test! {
    fn an_axes_map_session_at_version_4_is_refused_with_the_version_found() {
        let mut m = axes_map_meta();
        m.format_version = 4;
        let json = serde_json::to_string(&m).unwrap();
        assert!(matches!(
            SessionMeta::from_json(&json),
            Err(SessionError::UnsupportedFormatVersion { found: 4 })
        ));
    }
}

parity_test! {
    fn mounting_reads_toolhead_and_bed_and_null() {
        let mut value: serde_json::Value =
            serde_json::from_str(&capture_meta().to_json().unwrap()).unwrap();
        for (text, expected) in [
            (serde_json::json!("toolhead"), Some("toolhead".to_owned())),
            (serde_json::json!("bed"), Some("bed".to_owned())),
            (serde_json::Value::Null, None),
        ] {
            value["chips"][0]["mounting"] = text;
            let m = SessionMeta::from_json(&value.to_string()).unwrap();
            assert_eq!(m.chips[0].mounting, expected);
        }
    }
}

#[test]
fn legacy_mounting_strings_are_accepted_only_outside_calibration() {
    let mut ordinary = serde_json::to_value(capture_meta()).unwrap();
    ordinary["chips"][0]["mounting"] = serde_json::json!("frame");
    assert!(SessionMeta::from_json(&ordinary.to_string()).is_ok());
    let mut calibration = serde_json::to_value(axes_map_meta()).unwrap();
    calibration["chips"][0]["mounting"] = serde_json::json!("frame");
    assert!(SessionMeta::from_json(&calibration.to_string()).is_err());
}

fn vibrations_run(kinematics: &str) -> VibrationsRun {
    VibrationsRun {
        size: 100.0,
        z_height: 20.0,
        max_speed: 200.0,
        speed_increment: 2.0,
        accel: 3000.0,
        travel_speed: 120.0,
        min_speed: 20.0,
        kinematics: kinematics.to_owned(),
        max_velocity: None,
        angular_step: None,
    }
}

fn vibrations_meta(kinematics: &str, axes: [Direction; 2]) -> SessionMeta {
    let mut meta = vector_meta(&[(axes[0], 0), (axes[1], 0)]);
    meta.mode = None;
    meta.settings = None;
    meta.vibrations_run = Some(vibrations_run(kinematics));
    for (i, segment) in meta.segments.iter_mut().enumerate() {
        let base = 100.0 * i as f64;
        segment.window = Some(TimeWindow {
            start: base,
            end: base + 10.0,
        });
        segment.steps = vec![
            StepMeta {
                speed: 20.0,
                window: TimeWindow {
                    start: base + 1.0,
                    end: base + 4.0,
                },
            },
            StepMeta {
                speed: 22.0,
                window: TimeWindow {
                    start: base + 4.0,
                    end: base + 8.0,
                },
            },
        ];
    }
    meta
}

parity_test! {
    fn corexy_vibrations_session_is_vibrations_not_belts() {
        let meta = vibrations_meta("corexy", [Direction::BELT_B, Direction::BELT_A]);
        assert_eq!(meta.kind(), SessionKind::Vibrations);
        meta.to_json().unwrap();
        assert_eq!(vibrations_run("corexy").angles(), Some([45.0, 135.0]));
    }
}

parity_test! {
    fn cartesian_vibrations_session_is_vibrations_not_axes() {
        let mut meta = vibrations_meta("cartesian", [Direction::X, Direction::Y]);
        meta.format_version = 1;
        assert_eq!(meta.kind(), SessionKind::Vibrations);
        meta.to_json().unwrap();
        assert_eq!(vibrations_run("cartesian").angles(), Some([0.0, 90.0]));
        for kinematics in ["limited_cartesian", "corexz", "limited_corexz"] {
            assert_eq!(vibrations_run(kinematics).angles(), Some([0.0, 90.0]), "{kinematics}");
        }
        assert_eq!(vibrations_run("limited_corexy").angles(), Some([45.0, 135.0]));
        assert_eq!(vibrations_run("delta").angles(), None);
    }
}

parity_test! {
    fn old_session_without_new_fields_reads() {
        let m = capture_meta();
        let mut value: serde_json::Value = serde_json::from_str(&m.to_json().unwrap()).unwrap();
        value.as_object_mut().unwrap().remove("vibrations_run");
        for segment in value["segments"].as_array_mut().unwrap() {
            segment.as_object_mut().unwrap().remove("steps");
        }
        let old = SessionMeta::from_json(&value.to_string()).unwrap();
        assert_eq!(old.vibrations_run, None);
        assert!(old.segments.iter().all(|s| s.steps.is_empty()));
    }
}

parity_test! {
    fn steps_must_sit_inside_the_segment_window_and_not_overlap() {
        let mut outside = vibrations_meta("corexy", [Direction::BELT_B, Direction::BELT_A]);
        outside.segments[0].steps[1].window.end = 11.0;
        let err = outside.validate().unwrap_err();
        assert!(err.to_string().contains("segments[0].steps"), "{err}");

        let mut overlap = vibrations_meta("corexy", [Direction::BELT_B, Direction::BELT_A]);
        overlap.segments[0].steps[1].window.start = 3.0;
        let err = overlap.validate().unwrap_err();
        assert!(err.to_string().contains("segments[0].steps"), "{err}");

        let mut bad_speed = vibrations_meta("corexy", [Direction::BELT_B, Direction::BELT_A]);
        bad_speed.segments[0].steps[0].speed = 0.0;
        assert!(bad_speed.validate().is_err());

        let mut no_run = vibrations_meta("corexy", [Direction::BELT_B, Direction::BELT_A]);
        no_run.vibrations_run = None;
        assert!(no_run.validate().is_err());

        let mut three = vibrations_meta("corexy", [Direction::BELT_B, Direction::BELT_A]);
        three.segments.push(three.segments[0].clone());
        three.segments[2].point = 1;
        assert!(three.validate().is_err());
    }
}

fn stepped_meta(kinematics: &str, step: f64, angles: &[f64]) -> SessionMeta {
    let axes: Vec<Direction> = angles.iter().map(|&a| Direction::from_degrees(a)).collect();
    let mut meta = vibrations_meta("corexy", [Direction::BELT_B, Direction::BELT_A]);
    meta.format_version = 3;
    let template = meta.segments[0].clone();
    meta.segments = axes
        .iter()
        .enumerate()
        .map(|(i, &axis)| {
            let mut segment = template.clone();
            segment.axis = axis;
            segment.files = vec![file(0, i, 100)];
            segment
        })
        .collect();
    let mut run = vibrations_run(kinematics);
    run.angular_step = Some(step);
    meta.vibrations_run = Some(run);
    meta
}

fn stepped_run(kinematics: &str, step: Option<f64>) -> VibrationsRun {
    let mut run = vibrations_run(kinematics);
    run.angular_step = step;
    run
}

parity_test! {
    fn planned_angles_cartesian_every_45() {
        assert_eq!(
            stepped_run("cartesian", Some(45.0)).planned_angles(),
            Some(vec![0.0, 90.0, 45.0, 135.0])
        );
    }
}

parity_test! {
    fn planned_angles_corexy_every_45() {
        assert_eq!(
            stepped_run("corexy", Some(45.0)).planned_angles(),
            Some(vec![45.0, 135.0, 0.0, 90.0])
        );
    }
}

parity_test! {
    fn planned_angles_every_15_has_twelve_with_motor_pair_first() {
        let planned = stepped_run("corexy", Some(15.0)).planned_angles().unwrap();
        assert_eq!(planned.len(), 12);
        assert_eq!(planned[..2], [45.0, 135.0]);
        assert_eq!(planned[2..4], [0.0, 15.0]);
        assert_eq!(planned[11], 165.0);
    }
}

parity_test! {
    fn planned_angles_without_a_step_is_the_motor_pair() {
        assert_eq!(stepped_run("corexy", None).planned_angles(), Some(vec![45.0, 135.0]));
        assert_eq!(stepped_run("delta", None).planned_angles(), None);
        assert_eq!(stepped_run("delta", Some(45.0)).planned_angles(), None);
    }
}

parity_test! {
    fn a_step_needs_version_3() {
        let mut meta = stepped_meta("corexy", 45.0, &[45.0, 135.0]);
        assert_eq!(meta.required_version(), 3);
        meta.validate().unwrap();
        meta.format_version = 2;
        let err = meta.validate().unwrap_err();
        assert_eq!(invalid_field(err), "format_version");
    }
}

parity_test! {
    fn an_angle_direction_needs_version_3() {
        let mut meta = stepped_meta("corexy", 22.5, &[45.0, 135.0, 0.0, 22.5]);
        assert_eq!(meta.required_version(), 3);
        meta.format_version = 2;
        assert_eq!(invalid_field(meta.validate().unwrap_err()), "format_version");
        let mut plain = vector_meta(&[(Direction::Angle(22500), 0)]);
        assert_eq!(plain.required_version(), 3);
        assert_eq!(invalid_field(plain.validate().unwrap_err()), "format_version");
        plain.format_version = 3;
        assert_eq!(invalid_field(plain.validate().unwrap_err()), "segments[0].axis");
    }
}

parity_test! {
    fn an_unaccepted_step_is_refused() {
        let meta = stepped_meta("corexy", 30.0, &[45.0, 135.0]);
        assert_eq!(invalid_field(meta.validate().unwrap_err()), "vibrations_run.angular_step");
    }
}

parity_test! {
    fn an_angle_outside_the_steps_grid_is_refused() {
        let meta = stepped_meta("corexy", 45.0, &[45.0, 135.0, 0.0, 22.5]);
        assert_eq!(invalid_field(meta.validate().unwrap_err()), "segments[3].axis");
    }
}

parity_test! {
    fn a_non_canonical_label_is_refused() {
        let mut meta = stepped_meta("corexy", 45.0, &[45.0, 135.0]);
        meta.segments[0].axis = Direction::Angle(45000);
        assert_eq!(invalid_field(meta.validate().unwrap_err()), "segments[0].axis");
        let mut meta = stepped_meta("corexy", 45.0, &[45.0, 135.0]);
        meta.segments[0].axis = Direction::Vector([1, 2, 0]);
        assert_eq!(invalid_field(meta.validate().unwrap_err()), "segments[0].axis");
    }
}

parity_test! {
    fn a_duplicate_angle_is_refused() {
        let mut meta = stepped_meta("corexy", 45.0, &[45.0, 135.0]);
        meta.segments[1].axis = Direction::BELT_B;
        assert_eq!(invalid_field(meta.validate().unwrap_err()), "segments[1].axis");
    }
}

parity_test! {
    fn segments_must_follow_plan_order() {
        let meta = stepped_meta("corexy", 45.0, &[135.0, 45.0]);
        assert_eq!(invalid_field(meta.validate().unwrap_err()), "segments[0].axis");
        let meta = stepped_meta("corexy", 45.0, &[45.0, 135.0, 90.0, 0.0]);
        assert_eq!(invalid_field(meta.validate().unwrap_err()), "segments[2].axis");
        stepped_meta("corexy", 45.0, &[45.0, 135.0, 0.0, 90.0]).validate().unwrap();
    }
}

parity_test! {
    fn any_outcome_may_hold_a_prefix_of_the_plan_or_none() {
        for count in 0..=4 {
            let angles = [45.0, 135.0, 0.0, 90.0];
            let mut meta = stepped_meta("corexy", 45.0, &angles[..count]);
            for outcome in [
                Outcome::Complete,
                Outcome::Incomplete { reason: "stopped".into() },
                Outcome::Failed { reason: "failed".into() },
            ] {
                meta.outcome = outcome;
                meta.validate().unwrap();
            }
        }
    }
}

parity_test! {
    fn x_and_vector_labels_may_mix_only_when_a_step_is_set() {
        stepped_meta("cartesian", 45.0, &[0.0, 90.0, 45.0, 135.0]).validate().unwrap();
        let mut legacy = vibrations_meta("corexy", [Direction::X, Direction::BELT_A]);
        legacy.format_version = 2;
        assert_eq!(invalid_field(legacy.validate().unwrap_err()), "segments");
    }
}

parity_test! {
    fn a_legacy_vibrations_session_still_refuses_mixed_labels_and_a_third_segment() {
        let mut mixed = vibrations_meta("corexy", [Direction::X, Direction::BELT_A]);
        mixed.format_version = 2;
        assert!(mixed.validate().is_err());
        let mut three = vibrations_meta("corexy", [Direction::BELT_B, Direction::BELT_A]);
        three.segments.push(three.segments[0].clone());
        three.segments[2].axis = Direction::X;
        assert!(three.validate().is_err());
    }
}

parity_test! {
    fn a_motor_only_run_writes_no_angular_step() {
        let meta = vibrations_meta("corexy", [Direction::BELT_B, Direction::BELT_A]);
        let value: serde_json::Value = serde_json::from_str(&meta.to_json().unwrap()).unwrap();
        assert!(!value["vibrations_run"].as_object().unwrap().contains_key("angular_step"));
    }
}

parity_test! {
    fn version_4_is_unsupported_and_3_is_supported() {
        let meta = stepped_meta("corexy", 45.0, &[45.0, 135.0]);
        let json = meta.to_json().unwrap();
        SessionMeta::from_json(&json).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value["format_version"] = serde_json::json!(4);
        assert!(matches!(
            SessionMeta::from_json(&value.to_string()),
            Err(SessionError::UnsupportedFormatVersion { found: 4 })
        ));
    }
}

parity_test! {
    fn an_old_session_reads_as_the_motor_pair() {
        let meta = vibrations_meta("corexy", [Direction::BELT_B, Direction::BELT_A]);
        let mut value: serde_json::Value = serde_json::from_str(&meta.to_json().unwrap()).unwrap();
        value["vibrations_run"].as_object_mut().unwrap().remove("angular_step");
        let old = SessionMeta::from_json(&value.to_string()).unwrap();
        let run = old.vibrations_run.unwrap();
        assert_eq!(run.angular_step, None);
        assert_eq!(run.planned_angles(), Some(vec![45.0, 135.0]));
    }
}
