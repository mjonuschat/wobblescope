use wobblescope_core::RawSamples;
use wobblescope_core::fit::Axis;
use wobblescope_session::SessionError;
use wobblescope_session::import::{
    GroupRequest, ImportFile, ImportRequest, build_klipper_group, import_klipper_csv,
    parse_klipper_csv,
};
use wobblescope_session::meta::Source;

const POINT: [f64; 3] = [100.0, 100.0, 20.0];

fn request() -> GroupRequest {
    GroupRequest {
        id: "import-1".to_owned(),
        created_at: "2026-11-12T10:00:00Z".to_owned(),
    }
}

fn samples(t0: f64, n: usize, step: f64) -> RawSamples {
    let t: Vec<f64> = (0..n).map(|i| t0 + i as f64 * step).collect();
    RawSamples {
        x: vec![1.0; n],
        y: vec![2.0; n],
        z: vec![3.0; n],
        t,
    }
}

fn file(chip: &str, axis: Axis, point: Option<[f64; 3]>, t0: f64, n: usize) -> ImportFile {
    ImportFile {
        chip_name: chip.to_owned(),
        axis,
        point,
        file_name: Some(format!("raw_data_{chip}.csv")),
        samples: samples(t0, n, 0.001),
    }
}

#[test]
fn two_chips_on_one_axis_make_one_segment_with_two_files() {
    let group = build_klipper_group(
        &request(),
        vec![
            file("adxl345", Axis::X, None, 0.0, 100),
            file("lis2dw ebb36", Axis::X, None, 0.0, 100),
        ],
    )
    .unwrap();
    let names: Vec<&str> = group.meta.chips.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["adxl345", "lis2dw ebb36"]);
    assert_eq!(group.meta.segments.len(), 1);
    assert_eq!(group.meta.segments[0].files.len(), 2);
    assert!(group.meta.validate().is_ok());
    assert_eq!(group.sample_count(), 200);
}

#[test]
fn two_axes_make_two_segments_and_distinct_points_make_distinct_probe_points() {
    let group = build_klipper_group(
        &request(),
        vec![
            file("adxl345", Axis::X, None, 0.0, 10),
            file("adxl345", Axis::Y, Some(POINT), 0.0, 10),
        ],
    )
    .unwrap();
    assert_eq!(group.meta.segments.len(), 2);
    assert_eq!(group.meta.probe_points.len(), 2);
    assert!(group.meta.validate().is_ok());
}

#[test]
fn an_unknown_point_matches_only_an_unknown_point() {
    let group = build_klipper_group(
        &request(),
        vec![
            file("adxl345", Axis::X, None, 0.0, 10),
            file("adxl345", Axis::X, Some(POINT), 0.0, 10),
        ],
    )
    .unwrap();
    assert_eq!(group.meta.segments.len(), 2);

    let err = build_klipper_group(
        &request(),
        vec![
            file("adxl345", Axis::X, None, 0.0, 10),
            file("adxl345", Axis::X, None, 0.0, 10),
        ],
    )
    .unwrap_err();
    assert!(matches!(err, SessionError::ImportDuplicate { .. }));
}

#[test]
fn the_same_chip_axis_and_point_twice_is_refused_naming_them() {
    let err = build_klipper_group(
        &request(),
        vec![
            file("adxl345", Axis::Y, Some(POINT), 0.0, 10),
            file("adxl345", Axis::Y, Some(POINT), 0.0, 10),
        ],
    )
    .unwrap_err();
    match err {
        SessionError::ImportDuplicate { chip, axis, point } => {
            assert_eq!(chip, "adxl345");
            assert_eq!(axis, Axis::Y);
            assert_eq!(point, Some(POINT));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn chips_with_non_overlapping_time_ranges_in_one_segment_are_refused() {
    let apart = |a: f64, b: f64| {
        vec![
            ImportFile {
                samples: samples(a, 1001, 0.001),
                ..file("a", Axis::X, None, 0.0, 2)
            },
            ImportFile {
                samples: samples(b, 1001, 0.001),
                ..file("b", Axis::X, None, 0.0, 2)
            },
        ]
    };
    let err = build_klipper_group(&request(), apart(0.0, 5.0)).unwrap_err();
    assert!(matches!(err, SessionError::ImportNotSameRun { .. }));
    assert_eq!(
        err.to_string(),
        "these files were not recorded in the same run"
    );
    assert!(build_klipper_group(&request(), apart(0.0, 0.5)).is_ok());

    let other_axes = vec![
        file("a", Axis::X, None, 0.0, 100),
        file("b", Axis::Y, None, 500.0, 100),
    ];
    assert!(build_klipper_group(&request(), other_axes).is_ok());
}

#[test]
fn each_chip_keeps_its_own_observed_rate() {
    let mut slow = file("b", Axis::X, None, 0.0, 10);
    slow.samples = samples(0.0, 100, 0.002);
    let group =
        build_klipper_group(&request(), vec![file("a", Axis::X, None, 0.0, 100), slow]).unwrap();
    let rate = |i: usize| group.meta.chips[i].observed_rate_hz.unwrap();
    assert!((rate(0) - 1000.0).abs() < 1.0);
    assert!((rate(1) - 500.0).abs() < 1.0);
}

#[test]
fn the_group_note_about_an_unknown_point_appears_once() {
    let group = build_klipper_group(
        &request(),
        vec![
            file("a", Axis::X, None, 0.0, 10),
            file("b", Axis::X, None, 0.0, 10),
        ],
    )
    .unwrap();
    assert_eq!(group.meta.notes.len(), 1);

    let known =
        build_klipper_group(&request(), vec![file("a", Axis::X, Some(POINT), 0.0, 10)]).unwrap();
    assert!(known.meta.notes.is_empty());
}

#[test]
fn no_files_is_refused() {
    let err = build_klipper_group(&request(), Vec::new()).unwrap_err();
    assert!(matches!(err, SessionError::ImportNoFiles));
}

#[test]
fn take_returns_each_files_samples_by_sample_file_name_and_only_once() {
    let mut group = build_klipper_group(
        &request(),
        vec![
            file("a", Axis::X, None, 0.0, 10),
            file("b", Axis::X, None, 0.0, 20),
        ],
    )
    .unwrap();
    let files = group.meta.segments[0].files.clone();
    assert_eq!(group.take(&files[1]).unwrap().len(), 20);
    assert_eq!(group.take(&files[0]).unwrap().len(), 10);
    assert!(group.take(&files[0]).is_err());
}

#[test]
fn a_one_file_group_equals_the_single_file_import() {
    let text = "#time,accel_x,accel_y,accel_z\n1.0,1,2,3\n1.001,4,5,6\n1.002,7,8,9\n";
    let single = import_klipper_csv(
        text,
        &ImportRequest {
            id: "import-1".to_owned(),
            created_at: "2026-11-12T10:00:00Z".to_owned(),
            chip_name: "adxl345".to_owned(),
            axis: Axis::X,
            point: None,
            original_file_name: Some("raw_data_x.csv".to_owned()),
        },
    )
    .unwrap();
    let mut group = build_klipper_group(
        &request(),
        vec![ImportFile {
            chip_name: "adxl345".to_owned(),
            axis: Axis::X,
            point: None,
            file_name: Some("raw_data_x.csv".to_owned()),
            samples: parse_klipper_csv(text).unwrap(),
        }],
    )
    .unwrap();
    assert_eq!(group.meta, single.meta);
    let file = group.meta.segments[0].files[0].clone();
    assert_eq!(group.take(&file).unwrap(), single.samples);
}

#[test]
fn a_multi_file_group_lists_every_name_and_no_single_name() {
    let group = build_klipper_group(
        &request(),
        vec![
            file("a", Axis::X, None, 0.0, 10),
            file("b", Axis::X, None, 0.0, 10),
        ],
    )
    .unwrap();
    let Source::CsvImport {
        original_file_name,
        original_file_names,
    } = &group.meta.source
    else {
        panic!("not a CSV import");
    };
    assert_eq!(*original_file_name, None);
    assert_eq!(original_file_names, &["raw_data_a.csv", "raw_data_b.csv"]);
}

#[test]
fn bom_crlf_and_comment_files_group_like_single_imports() {
    let bom = "\u{feff}#time,accel_x,accel_y,accel_z\r\n1.0,1,2,3\r\n1.001,4,5,6\r\n";
    let comments = "# a comment\n\n1.0,1,2,3\n# mid\n1.001,4,5,6\n";
    let mk = |chip: &str, text: &str| ImportFile {
        chip_name: chip.to_owned(),
        axis: Axis::X,
        point: None,
        file_name: None,
        samples: parse_klipper_csv(text).unwrap(),
    };
    let group = build_klipper_group(&request(), vec![mk("a", bom), mk("b", comments)]).unwrap();
    assert!(group.meta.validate().is_ok());
    assert_eq!(group.meta.segments[0].files.len(), 2);
}
