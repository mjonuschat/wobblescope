mod common;

use std::path::Path;

use common::{Spec, probe, seal_in, sessions_dir, settings};
use tempfile::TempDir;
use time::OffsetDateTime;
use wobblescope::csv_import::{ImportError, ImportOptions, import_csv};
use wobblescope::storage::index::{ListFilter, SourceKind};
use wobblescope::storage::{Storage, StorageError};
use wobblescope_core::fit::Axis;

const BIG: u64 = u64::MAX / 4;
const NOON: i64 = 1_793_448_000;

fn at(unix: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(unix).unwrap()
}

const CSV: &str = "#time,accel_x,accel_y,accel_z\n\
                   100.000000,1.0,2.0,3.0\n\
                   100.000313,1.5,2.5,3.5\n\
                   100.000626,2.0,3.0,4.0\n";

fn opts(chip: &str) -> ImportOptions<'_> {
    ImportOptions {
        chip,
        axis: Some(Axis::X),
        point: None,
        file_name: None,
    }
}

fn open(data: &Path, cap: u64) -> Storage {
    Storage::open(data, &settings(cap, 0), probe(BIG))
        .unwrap()
        .0
}

fn ids(s: &Storage) -> Vec<String> {
    let mut v: Vec<String> = s
        .list(&ListFilter::default())
        .into_iter()
        .map(|x| x.id)
        .collect();
    v.sort();
    v
}

fn dirs(data: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(sessions_dir(data))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// Four equal sessions, s1 the oldest, and a storage whose cap they exactly fill.
fn full(data: &Path) -> Storage {
    for (i, day) in ["01", "02", "03", "04"].iter().enumerate() {
        seal_in(
            data,
            &Spec::new(&format!("s{}", i + 1), &format!("2026-10-{day}T10:00:00Z")),
        );
    }
    let total = open(data, BIG).usage().unwrap().used_bytes;
    open(data, total)
}

#[test]
fn a_csv_becomes_a_listed_import_session_named_for_the_time() {
    let dir = TempDir::new().unwrap();
    let s = open(dir.path(), BIG);
    let summary = import_csv(&s, CSV.as_bytes(), &opts("adxl345"), at(NOON)).unwrap();
    assert_eq!(summary.id, "csv-20261031T120000Z");
    assert_eq!(summary.created_at, "2026-10-31T12:00:00Z");
    assert_eq!(summary.source, SourceKind::CsvImport);
    assert_eq!(summary.chips, ["adxl345"]);
    assert_eq!(ids(&s), ["csv-20261031T120000Z"]);
    let (_, meta) = s.session(&summary.id).unwrap();
    assert_eq!(meta.id, summary.id);
}

#[test]
fn a_second_import_in_the_same_second_gets_a_suffix() {
    let dir = TempDir::new().unwrap();
    let s = open(dir.path(), BIG);
    import_csv(&s, CSV.as_bytes(), &opts("adxl345"), at(NOON)).unwrap();
    let second = import_csv(&s, CSV.as_bytes(), &opts("adxl345"), at(NOON)).unwrap();
    assert_eq!(second.id, "csv-20261031T120000Z-2");
    let (_, meta) = s.session(&second.id).unwrap();
    assert_eq!(meta.id, "csv-20261031T120000Z-2");
}

#[test]
fn axis_and_point_come_from_the_file_name_when_not_given() {
    let dir = TempDir::new().unwrap();
    let s = open(dir.path(), BIG);
    let name = "raw_data_y_adxl345_100.000_100.000_20.000_test.csv";
    let o = ImportOptions {
        chip: "adxl345",
        axis: None,
        point: None,
        file_name: Some(name),
    };
    let summary = import_csv(&s, CSV.as_bytes(), &o, at(NOON)).unwrap();
    let (_, meta) = s.session(&summary.id).unwrap();
    assert_eq!(meta.segments[0].axis, Axis::Y);
    assert_eq!(meta.probe_points[0].position, Some([100.0, 100.0, 20.0]));
}

#[test]
fn an_explicit_axis_wins_over_the_file_name() {
    let dir = TempDir::new().unwrap();
    let s = open(dir.path(), BIG);
    let o = ImportOptions {
        chip: "adxl345",
        axis: Some(Axis::X),
        point: None,
        file_name: Some("raw_data_y_adxl345_100.000_100.000_20.000_test.csv"),
    };
    let summary = import_csv(&s, CSV.as_bytes(), &o, at(NOON)).unwrap();
    let (_, meta) = s.session(&summary.id).unwrap();
    assert_eq!(meta.segments[0].axis, Axis::X);
}

#[test]
fn no_axis_anywhere_is_refused_before_anything_is_admitted() {
    let dir = TempDir::new().unwrap();
    let s = open(dir.path(), BIG);
    let o = ImportOptions {
        chip: "adxl345",
        axis: None,
        point: None,
        file_name: Some("capture.csv"),
    };
    assert!(matches!(
        import_csv(&s, CSV.as_bytes(), &o, at(NOON)),
        Err(ImportError::MissingAxis)
    ));
    assert_eq!(s.usage().unwrap().reserved_bytes, 0);
    assert!(dirs(dir.path()).is_empty());
}

#[test]
fn an_empty_chip_name_is_refused() {
    let dir = TempDir::new().unwrap();
    let s = open(dir.path(), BIG);
    assert!(matches!(
        import_csv(&s, CSV.as_bytes(), &opts(""), at(NOON)),
        Err(ImportError::MissingChip)
    ));
}

#[test]
fn bad_csv_deletes_nothing_even_with_a_full_cap() {
    let dir = TempDir::new().unwrap();
    let s = full(dir.path());
    let bad_utf8: &[u8] = &[0xff, 0xfe, b'\n'];
    let bad_row = "#t\n1.0,1,2,3\n2.0,1,2,3\n3.0,1\n";
    for body in [bad_utf8, bad_row.as_bytes()] {
        assert!(matches!(
            import_csv(&s, body, &opts("adxl345"), at(NOON)),
            Err(ImportError::InvalidCsv(_))
        ));
    }
    assert_eq!(ids(&s), ["s1", "s2", "s3", "s4"]);
    let msg = match import_csv(&s, bad_row.as_bytes(), &opts("adxl345"), at(NOON)) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("accepted"),
    };
    assert!(msg.contains("line 4"), "{msg}");
}

#[test]
fn with_everything_pinned_a_valid_import_is_refused_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let s = full(dir.path());
    for id in ["s1", "s2", "s3", "s4"] {
        s.pin(id).unwrap();
    }
    assert!(matches!(
        import_csv(&s, CSV.as_bytes(), &opts("adxl345"), at(NOON)),
        Err(ImportError::Storage(StorageError::CapExceeded { .. }))
    ));
    assert_eq!(dirs(dir.path()), ["s1", "s2", "s3", "s4"]);
}

#[test]
fn a_full_cap_of_unpinned_sessions_loses_its_oldest_to_the_import() {
    let dir = TempDir::new().unwrap();
    let s = full(dir.path());
    let summary = import_csv(&s, CSV.as_bytes(), &opts("adxl345"), at(NOON)).unwrap();
    assert_eq!(ids(&s), ["csv-20261031T120000Z", "s2", "s3", "s4"]);
    assert_eq!(summary.id, "csv-20261031T120000Z");
}

#[test]
fn a_real_capture_imports_and_loads_back_with_its_sample_count() {
    use std::io::Read;
    let bytes = include_bytes!("../../core/tests/data/real/win_x_short_adxl345.csv.zst");
    let mut text = String::new();
    structured_zstd::decoding::StreamingDecoder::new(&bytes[..])
        .read_to_string(&mut text)
        .unwrap();
    let rows = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .count();

    let dir = TempDir::new().unwrap();
    let s = open(dir.path(), BIG);
    let summary = import_csv(&s, text.as_bytes(), &opts("adxl345"), at(NOON)).unwrap();
    let mut file = s.open_sample(&summary.id, 0, 0).unwrap();
    let mut stored = Vec::new();
    file.read_to_end(&mut stored).unwrap();
    let decoded = wobblescope_session::codec::decode_samples(&stored).unwrap();
    assert_eq!(decoded.len(), rows);
}
