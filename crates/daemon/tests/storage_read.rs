mod common;

use std::fs;
use std::io::Read;

use common::{Spec, open, seal_in, sessions_dir, size_of};
use tempfile::TempDir;
use wobblescope::storage::StorageError;

const BIG: u64 = u64::MAX / 4;

#[test]
fn session_returns_the_summary_and_the_stored_metadata() {
    let dir = TempDir::new().unwrap();
    let spec = Spec::new("s1", "2026-10-01T10:00:00Z").chips(&["adxl345", "ebb36"]);
    seal_in(dir.path(), &spec);
    let (s, _) = open(dir.path(), BIG);
    let (summary, meta) = s.session("s1").unwrap();
    assert_eq!(meta, spec.meta());
    assert_eq!(summary, s.get("s1").unwrap());
}

#[test]
fn session_of_an_unknown_or_skipped_id_is_not_found() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("newer", "2026-10-01T10:00:00Z"));
    let p = sessions_dir(dir.path()).join("newer").join("meta.json");
    let text = fs::read_to_string(&p)
        .unwrap()
        .replace("\"format_version\": 1", "\"format_version\": 3");
    fs::write(&p, text).unwrap();
    let (s, report) = open(dir.path(), BIG);
    assert_eq!(report.skipped.len(), 1);
    assert!(matches!(s.session("nope"), Err(StorageError::NotFound(_))));
    assert!(matches!(s.session("newer"), Err(StorageError::NotFound(_))));
}

#[test]
fn session_whose_directory_vanished_is_not_found_not_an_io_error() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    fs::remove_dir_all(sessions_dir(dir.path()).join("s1")).unwrap();
    assert!(matches!(s.session("s1"), Err(StorageError::NotFound(_))));
}

#[test]
fn a_held_sample_guard_blocks_delete_and_eviction_and_reads_the_file_on_disk() {
    let dir = TempDir::new().unwrap();
    let spec = |id: &str, day: &str| {
        Spec::new(id, &format!("2026-10-{day}T10:00:00Z")).chips(&["adxl345", "ebb36"])
    };
    seal_in(dir.path(), &spec("s1", "01"));
    seal_in(dir.path(), &spec("s2", "02"));
    let (probe_storage, _) = open(dir.path(), BIG);
    let one = size_of(&probe_storage, "s1");
    let (s, _) = wobblescope::storage::Storage::open(
        dir.path(),
        &common::settings(2 * one, 0),
        common::probe(BIG),
    )
    .unwrap();

    let mut guard = s.open_sample("s1", 1, 0).unwrap();
    let mut bytes = Vec::new();
    guard.read_to_end(&mut bytes).unwrap();
    assert_eq!(
        bytes,
        fs::read(sessions_dir(dir.path()).join("s1/c1-s0.zst")).unwrap()
    );
    assert!(matches!(s.delete("s1"), Err(StorageError::InUse(_))));

    let reservation = s.admit("s3", one).unwrap();
    assert!(s.get("s1").is_some());
    assert!(s.get("s2").is_none());
    drop(reservation);

    drop(guard);
    s.delete("s1").unwrap();
}

#[test]
fn an_unrecorded_pair_is_no_recording_and_leaves_no_lease() {
    let dir = TempDir::new().unwrap();
    seal_in(
        dir.path(),
        &Spec::new("s1", "2026-10-01T10:00:00Z").chips(&["adxl345"]),
    );
    let (s, _) = open(dir.path(), BIG);
    for (chip, segment) in [(1, 0), (0, 1)] {
        match s.open_sample("s1", chip, segment) {
            Err(StorageError::NoRecording {
                id,
                chip: c,
                segment: g,
            }) => assert_eq!((id.as_str(), c, g), ("s1", chip, segment)),
            other => panic!("expected NoRecording, got {:?}", other.err()),
        }
    }
    s.delete("s1").unwrap();
}

#[test]
fn open_sample_of_an_unknown_id_is_not_found() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    assert!(matches!(
        s.open_sample("nope", 0, 0),
        Err(StorageError::NotFound(_))
    ));
}
