#[macro_use]
mod common;

use common::{parse_csv, real, real_text};
use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::{Coverage, DatasetInput, Health};
use wobblescope_core::fit::{Axis, FitParams};
use wobblescope_core::multichip::analyze;
use wobblescope_session::SessionError;
use wobblescope_session::bridge::dataset_inputs;
use wobblescope_session::codec::{decode_samples, encode_samples};
use wobblescope_session::import::{
    ImportRequest, import_klipper_csv, suggest_hints_from_file_name,
};
use wobblescope_session::meta::{Outcome, Source};

fn request(chip: &str) -> ImportRequest {
    ImportRequest {
        id: "import-1".to_owned(),
        created_at: "2026-10-31T19:00:00Z".to_owned(),
        chip_name: chip.to_owned(),
        axis: Axis::X,
        point: None,
        original_file_name: Some("raw_data_x_ws.csv".to_owned()),
    }
}

const HEADER: &str = "#time,accel_x,accel_y,accel_z";
const ROWS: [&str; 3] = [
    "41230.322878,148.041188,296.082377,-9806.022374",
    "41230.323191,222.061783,0.000000,-9729.412825",
    "41230.323504,370.102971,-222.061783,-9269.755526",
];

fn text_with(header: bool, sep: &str, trailing: bool) -> String {
    let mut lines: Vec<&str> = vec![];
    if header {
        lines.push(HEADER);
    }
    lines.extend(ROWS);
    let mut t = lines.join(sep);
    if trailing {
        t.push_str(sep);
    }
    t
}

fn line_of(err: SessionError) -> usize {
    match err {
        SessionError::CsvRow { line, .. } => line,
        other => panic!("expected CsvRow, got {other:?}"),
    }
}

parity_test! {
    fn real_captures_import_equal_to_the_reference_parse() {
        for name in ["ws_x_adxl345", "ws_x_ebb36"] {
            let imp = import_klipper_csv(&real_text(name), &request("c")).unwrap();
            assert_eq!(imp.samples, real(name), "{name}");
        }
    }
}

parity_test! {
    fn an_imported_session_analyses_like_direct_input() {
        let text = real_text("ws_x_adxl345");
        let imp = import_klipper_csv(&text, &request("adxl345")).unwrap();
        let stored = encode_samples(&imp.samples).unwrap();
        let inputs = dataset_inputs(&imp.meta, Axis::X, |_| decode_samples(&stored)).unwrap();
        let direct = vec![DatasetInput {
            chip: "adxl345".to_owned(),
            point: 0,
            samples: real("ws_x_adxl345"),
            health: Health::Unknown,
            coverage: Coverage::Complete,
            include_override: false,
            window: None,
            exclusions: Vec::new(),
            load_error: None,
        }];
        let params = FitParams::default();
        assert_eq!(
            analyze(&inputs, &params).unwrap(),
            analyze(&direct, &params).unwrap()
        );
    }
}

parity_test! {
    fn six_decimal_values_survive_import_encode_and_decode_bit_for_bit() {
        let imp = import_klipper_csv(&text_with(true, "\n", true), &request("c")).unwrap();
        let back = decode_samples(&encode_samples(&imp.samples).unwrap()).unwrap();
        let want = parse_csv(&text_with(true, "\n", true));
        for (a, b) in [(&back.t, &want.t), (&back.x, &want.x), (&back.y, &want.y), (&back.z, &want.z)] {
            for (u, v) in a.iter().zip(b) {
                assert_eq!(u.to_bits(), v.to_bits());
            }
        }
        assert_eq!(back.t[0].to_bits(), "41230.322878".parse::<f64>().unwrap().to_bits());
        assert_eq!(format!("{:.6}", back.z[0]), "-9806.022374");
    }
}

parity_test! {
    fn the_imported_meta_describes_an_unknown_provenance() {
        let imp = import_klipper_csv(&text_with(true, "\n", true), &request("lis2dw ebb36")).unwrap();
        let m = &imp.meta;
        assert_eq!(m.chips.len(), 1);
        assert_eq!(m.chips[0].name, "lis2dw ebb36");
        assert_eq!(m.chips[0].health, Health::Unknown);
        assert!(m.chips[0].chip_type.is_none() && m.chips[0].nominal_rate_hz.is_none());
        let step = imp.samples.t[1] - imp.samples.t[0];
        let rate = m.chips[0].observed_rate_hz.unwrap();
        assert!((rate - 1.0 / step).abs() < 1e-6 * rate, "{rate}");
        assert_eq!(m.outcome, Outcome::Complete);
        assert_eq!(m.segments.len(), 1);
        assert_eq!(m.segments[0].axis, Axis::X);
        assert!(m.segments[0].window.is_none());
        assert_eq!(m.segments[0].files[0].coverage, Coverage::Complete);
        assert_eq!(m.segments[0].files[0].sample_count, 3);
        assert_eq!(
            m.source,
            Source::CsvImport { original_file_name: Some("raw_data_x_ws.csv".to_owned()) }
        );
        assert!(m.notes.iter().any(|n| n.contains("probe point is unknown")));
        assert_eq!(m.probe_points[0].position, None);
    }
}

parity_test! {
    fn the_observed_rate_is_the_median_interval_not_the_span() {
        // 3200 Hz rows with one 50 ms hole: the span-based rate would be about 2.6 kHz.
        let mut text = String::new();
        let mut t = 100.0;
        for i in 0..1000 {
            if i == 500 {
                t += 0.05;
            }
            text.push_str(&format!("{t:.6},1.0,2.0,3.0\n"));
            t += 1.0 / 3200.0;
        }
        let imp = import_klipper_csv(&text, &request("c")).unwrap();
        let rate = imp.meta.chips[0].observed_rate_hz.unwrap();
        assert!((rate - 3200.0).abs() < 16.0, "{rate}");
    }
}

parity_test! {
    fn a_point_hint_is_recorded_and_the_unknown_note_is_dropped() {
        let mut req = request("c");
        req.point = Some([120.0, 115.5, 50.0]);
        let imp = import_klipper_csv(&text_with(true, "\n", true), &req).unwrap();
        assert_eq!(imp.meta.probe_points[0].position, Some([120.0, 115.5, 50.0]));
        assert!(imp.meta.notes.iter().all(|n| !n.contains("unknown")));
    }
}

parity_test! {
    fn format_variants_yield_identical_samples() {
        let base = import_klipper_csv(&text_with(true, "\n", true), &request("c")).unwrap().samples;
        let variants = [
            text_with(false, "\n", true),
            text_with(true, "\r\n", true),
            format!("\u{feff}{}", text_with(true, "\n", true)),
            format!("# a comment\n{}", text_with(true, "\n", true)),
            text_with(true, "\n", false),
            text_with(true, "\n\n", true),
        ];
        for (i, v) in variants.iter().enumerate() {
            let got = import_klipper_csv(v, &request("c")).unwrap().samples;
            assert_eq!(got, base, "variant {i}");
        }
    }
}

parity_test! {
    fn malformed_rows_are_rejected_with_their_line() {
        let cases: [(&str, usize); 8] = [
            ("1.0,2.0,3.0\n", 5),
            ("1.0,2.0,3.0,4.0,5.0\n", 5),
            ("abc,2.0,3.0,4.0\n", 5),
            ("1.0,,3.0,4.0\n", 5),
            ("41231.0,NaN,3.0,4.0\n", 5),
            ("41231.0,inf,3.0,4.0\n", 5),
            ("41230.323504,1.0,2.0,3.0\n", 5),
            ("41230.0,1.0,2.0,3.0\n", 5),
        ];
        for (bad, line) in cases {
            let text = format!("{HEADER}\n{}\n{}\n{}\n{bad}", ROWS[0], ROWS[1], ROWS[2]);
            let err = import_klipper_csv(&text, &request("c")).unwrap_err();
            assert_eq!(line_of(err), line, "{bad:?}");
        }
    }
}

parity_test! {
    fn too_few_samples_are_rejected() {
        let one = format!("{HEADER}\n{}\n", ROWS[0]);
        assert!(matches!(
            import_klipper_csv(&one, &request("c")),
            Err(SessionError::CsvTooShort { found: 1 })
        ));
        let none = format!("{HEADER}\n");
        assert!(matches!(
            import_klipper_csv(&none, &request("c")),
            Err(SessionError::CsvTooShort { found: 0 })
        ));
    }
}

parity_test! {
    fn an_invalid_session_id_is_rejected() {
        let mut req = request("c");
        req.id = "a b".to_owned();
        assert!(matches!(
            import_klipper_csv(&text_with(true, "\n", true), &req),
            Err(SessionError::Invalid { .. })
        ));
    }
}

parity_test! {
    fn file_names_give_the_axis_and_the_point_when_present() {
        let h = suggest_hints_from_file_name("raw_data_x_ws.csv");
        assert_eq!((h.axis, h.point), (Some(Axis::X), None));

        let h = suggest_hints_from_file_name("raw_data_y_lis2dw_ebb36_120.000_115.500_50.000_my_name.csv");
        assert_eq!((h.axis, h.point), (Some(Axis::Y), Some([120.0, 115.5, 50.0])));

        let h = suggest_hints_from_file_name("raw_data_x_0.000_-5.250_10.000_ws.csv");
        assert_eq!((h.axis, h.point), (Some(Axis::X), Some([0.0, -5.25, 10.0])));

        let h = suggest_hints_from_file_name("/tmp/raw_data_x_ws.csv");
        assert_eq!((h.axis, h.point), (Some(Axis::X), None));

        let h = suggest_hints_from_file_name("raw_data_z_ws.csv");
        assert_eq!((h.axis, h.point), (None, None));

        let h = suggest_hints_from_file_name("resonances_x_ws.csv");
        assert_eq!((h.axis, h.point), (None, None));
    }
}

#[cfg(all(feature = "fs", not(target_arch = "wasm32")))]
#[test]
fn an_imported_session_survives_a_directory_round_trip() {
    use wobblescope_session::fs::{open_session, write_session};

    let root = tempfile::TempDir::new().unwrap();
    let text = real_text("ws_x_adxl345");
    let imp = import_klipper_csv(&text, &request("adxl345")).unwrap();
    write_session(root.path(), &imp.meta, |_| Ok(imp.samples.clone())).unwrap();
    let session = open_session(root.path(), "import-1").unwrap();
    let inputs = dataset_inputs(session.meta(), Axis::X, |f| session.load(f)).unwrap();
    let direct = dataset_inputs(&imp.meta, Axis::X, |_| {
        Ok::<RawSamples, SessionError>(imp.samples.clone())
    })
    .unwrap();
    let params = FitParams::default();
    assert_eq!(
        analyze(&inputs, &params).unwrap(),
        analyze(&direct, &params).unwrap()
    );
}
