#[macro_use]
mod common;

use wobblescope_core::eligibility::{Coverage, Health};
use wobblescope_core::fit::Axis;
use wobblescope_session::SessionError;
use wobblescope_session::codec::MAX_SAMPLES;
use wobblescope_session::meta::{
    ChipMeta, Mode, Outcome, PrinterInfo, ProbePoint, SegmentFile, SegmentMeta, SessionMeta,
    Source, SweepSettings, TimeDomain, TimeWindow, sample_file_name, validate_session_id,
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
                axis: Axis::X,
                point: 0,
                window: Some(TimeWindow {
                    start: 41230.322878,
                    end: 41262.104211,
                }),
                files: vec![file(0, 0, 100_000), file(1, 0, 12_000)],
            },
            SegmentMeta {
                axis: Axis::Y,
                point: 0,
                window: None,
                files: vec![file(0, 1, 100_000), file(1, 1, 12_000)],
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
        assert_eq!(meta.chips[0].health, Health::Unknown);
    }
}

parity_test! {
    fn a_newer_format_version_is_reported_even_when_the_shape_changed() {
        let err = SessionMeta::from_json(r#"{"format_version": 2, "renamed": true}"#).unwrap_err();
        assert!(matches!(err, SessionError::UnsupportedFormatVersion { found: 2 }));
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
        dup_segment.segments[1].axis = Axis::X;
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
