mod common;

use std::fs;
use std::path::Path;

use common::{Spec, seal};
use tempfile::TempDir;
use wobblescope::storage::index::{ListFilter, OutcomeKind, SourceKind, scan};
use wobblescope::storage::pins::Pins;
use wobblescope_session::fs::open_session;
use wobblescope_session::meta::{Outcome, Source};

fn ids(index: &wobblescope::storage::index::Index, filter: &ListFilter) -> Vec<String> {
    index.list(filter).into_iter().map(|s| s.id).collect()
}

fn scan_dir(
    root: &Path,
    pins: &Pins,
) -> (wobblescope::storage::index::Index, Vec<(String, String)>) {
    let (index, skipped) = scan(root, pins).unwrap();
    (
        index,
        skipped.into_iter().map(|s| (s.name, s.reason)).collect(),
    )
}

fn dir_bytes(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().to_string_lossy().into_owned(),
                fs::read(e.path()).unwrap(),
            )
        })
        .collect();
    v.sort();
    v
}

fn three_days(root: &Path) {
    seal(
        root,
        &Spec::new("d1", "2026-10-01T10:00:00Z")
            .chips(&["adxl345 nh36", "lis2dw ebb36"])
            .printer("voron"),
    );
    seal(
        root,
        &Spec::new("d2", "2026-10-02T10:00:00Z").chips(&["adxl345 nh36"]),
    );
    seal(
        root,
        &Spec::new("d3", "2026-10-03T10:00:00Z").printer("voron"),
    );
}

#[test]
fn sessions_list_newest_first() {
    let root = TempDir::new().unwrap();
    three_days(root.path());
    let (index, skipped) = scan_dir(root.path(), &Pins::default());
    assert!(skipped.is_empty());
    assert_eq!(ids(&index, &ListFilter::default()), ["d3", "d2", "d1"]);
}

#[test]
fn the_time_range_includes_its_start_and_excludes_its_end() {
    let root = TempDir::new().unwrap();
    three_days(root.path());
    let (index, _) = scan_dir(root.path(), &Pins::default());
    let t = |s| wobblescope::storage::index::created_at_unix(s).unwrap();
    let f = ListFilter {
        created_from: Some(t("2026-10-02T10:00:00Z")),
        created_to: Some(t("2026-10-03T10:00:00Z")),
        ..ListFilter::default()
    };
    assert_eq!(ids(&index, &f), ["d2"]);
    let f = ListFilter {
        created_from: Some(t("2026-10-02T00:00:00Z")),
        created_to: Some(t("2026-10-03T00:00:00Z")),
        ..ListFilter::default()
    };
    assert_eq!(ids(&index, &f), ["d2"]);
}

#[test]
fn the_chip_filter_matches_any_chip_of_the_session() {
    let root = TempDir::new().unwrap();
    three_days(root.path());
    let (index, _) = scan_dir(root.path(), &Pins::default());
    let f = ListFilter {
        chip: Some("adxl345 nh36".to_owned()),
        ..ListFilter::default()
    };
    assert_eq!(ids(&index, &f), ["d2", "d1"]);
    let f = ListFilter {
        chip: Some("lis2dw ebb36".to_owned()),
        ..ListFilter::default()
    };
    assert_eq!(ids(&index, &f), ["d1"]);
}

#[test]
fn the_printer_filter_never_matches_a_session_without_printer_info() {
    let root = TempDir::new().unwrap();
    three_days(root.path());
    let (index, _) = scan_dir(root.path(), &Pins::default());
    let f = ListFilter {
        printer: Some("voron".to_owned()),
        ..ListFilter::default()
    };
    assert_eq!(ids(&index, &f), ["d3", "d1"]);
}

#[test]
fn filters_combine_with_and() {
    let root = TempDir::new().unwrap();
    three_days(root.path());
    let mut pins = Pins::default();
    pins.insert("d1".to_owned());
    pins.insert("d2".to_owned());
    let (index, _) = scan_dir(root.path(), &pins);
    let f = ListFilter {
        pinned: Some(true),
        chip: Some("lis2dw ebb36".to_owned()),
        ..ListFilter::default()
    };
    assert_eq!(ids(&index, &f), ["d1"]);
    let f = ListFilter {
        pinned: Some(false),
        ..ListFilter::default()
    };
    assert_eq!(ids(&index, &f), ["d3"]);
}

#[test]
fn a_failed_import_summarizes_its_source_outcome_and_size() {
    let root = TempDir::new().unwrap();
    let mut spec = Spec::new("imp", "2026-10-01T10:00:00Z");
    spec.source = Source::CsvImport {
        original_file_name: Some("raw_data_x.csv".to_owned()),
        original_file_names: vec!["raw_data_x.csv".to_owned()],
    };
    spec.outcome = Outcome::Failed {
        reason: "disk".to_owned(),
    };
    seal(root.path(), &spec);
    let (index, _) = scan_dir(root.path(), &Pins::default());
    let s = index.get("imp").unwrap();
    assert_eq!(s.source, SourceKind::CsvImport);
    assert_eq!(s.outcome, OutcomeKind::Failed);
    assert_eq!(s.mode, None);
    assert_eq!(
        s.size_bytes,
        open_session(root.path(), "imp")
            .unwrap()
            .size_bytes()
            .unwrap()
    );
}

#[test]
fn offsets_are_honoured_when_ordering() {
    let root = TempDir::new().unwrap();
    seal(
        root.path(),
        &Spec::new("plus2", "2026-10-02T12:00:00+02:00"),
    );
    seal(root.path(), &Spec::new("utc", "2026-10-02T11:00:00Z"));
    let (index, _) = scan_dir(root.path(), &Pins::default());
    assert_eq!(ids(&index, &ListFilter::default()), ["utc", "plus2"]);
}

#[test]
fn equal_times_are_ordered_by_id() {
    let root = TempDir::new().unwrap();
    seal(root.path(), &Spec::new("b", "2026-10-02T10:00:00Z"));
    seal(root.path(), &Spec::new("a", "2026-10-02T10:00:00Z"));
    let (index, _) = scan_dir(root.path(), &Pins::default());
    assert_eq!(ids(&index, &ListFilter::default()), ["a", "b"]);
}

#[test]
fn unusable_sessions_are_skipped_with_a_reason_and_left_alone() {
    let root = TempDir::new().unwrap();
    seal(root.path(), &Spec::new("ok", "2026-10-01T10:00:00Z"));

    seal(root.path(), &Spec::new("newer", "2026-10-01T10:00:00Z"));
    let p = root.path().join("newer").join("meta.json");
    let text = fs::read_to_string(&p)
        .unwrap()
        .replace("\"format_version\": 1", "\"format_version\": 2");
    fs::write(&p, text).unwrap();

    seal(root.path(), &Spec::new("badtime", "yesterday"));

    fs::create_dir(root.path().join("junk")).unwrap();
    fs::write(root.path().join("junk").join("meta.json"), "{not json").unwrap();

    seal(root.path(), &Spec::new("s2", "2026-10-01T10:00:00Z"));
    fs::rename(root.path().join("s2"), root.path().join("s1")).unwrap();

    let before: Vec<_> = ["newer", "badtime", "junk", "s1"]
        .iter()
        .map(|n| dir_bytes(&root.path().join(n)))
        .collect();
    let (index, skipped) = scan_dir(root.path(), &Pins::default());
    let after: Vec<_> = ["newer", "badtime", "junk", "s1"]
        .iter()
        .map(|n| dir_bytes(&root.path().join(n)))
        .collect();
    assert_eq!(before, after);

    assert_eq!(ids(&index, &ListFilter::default()), ["ok"]);
    let reason = |name: &str| skipped.iter().find(|(n, _)| n == name).unwrap().1.clone();
    assert!(reason("newer").contains('2'), "{}", reason("newer"));
    assert!(
        reason("badtime").contains("created_at"),
        "{}",
        reason("badtime")
    );
    assert!(!reason("junk").is_empty());
    assert!(reason("s1").contains("s2"), "{}", reason("s1"));
    assert_eq!(skipped.len(), 4);
    assert_eq!(index.used_bytes(), index.get("ok").unwrap().size_bytes);
}

#[test]
fn the_scan_does_not_decode_samples() {
    let root = TempDir::new().unwrap();
    seal(root.path(), &Spec::new("cut", "2026-10-01T10:00:00Z"));
    let p = root.path().join("cut").join("c0-s0.zst");
    let mut bytes = fs::read(&p).unwrap();
    bytes.truncate(10);
    fs::write(&p, bytes).unwrap();
    let (index, skipped) = scan_dir(root.path(), &Pins::default());
    assert!(skipped.is_empty());
    assert!(index.get("cut").is_some());
}

#[test]
fn usage_numbers_add_up() {
    let root = TempDir::new().unwrap();
    three_days(root.path());
    let mut pins = Pins::default();
    pins.insert("d2".to_owned());
    let (index, _) = scan_dir(root.path(), &pins);
    let all: u64 = ["d1", "d2", "d3"]
        .iter()
        .map(|i| index.get(i).unwrap().size_bytes)
        .sum();
    assert_eq!(index.used_bytes(), all);
    assert_eq!(index.pinned_bytes(), index.get("d2").unwrap().size_bytes);
    assert_eq!(index.len(), 3);
}

#[test]
fn an_empty_sessions_directory_gives_an_empty_index() {
    let root = TempDir::new().unwrap();
    let (index, skipped) = scan_dir(root.path(), &Pins::default());
    assert!(skipped.is_empty());
    assert_eq!(index.len(), 0);
    assert_eq!(index.used_bytes(), 0);
}
