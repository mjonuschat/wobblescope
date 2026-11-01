mod common;

use std::fs;
use std::path::Path;

use common::{Spec, open, samples, seal_in, sessions_dir};
use tempfile::TempDir;
use wobblescope::capture::session_id::{base_id, first_free_id};
use wobblescope::storage::index::{ListFilter, OutcomeKind};
use wobblescope::storage::{Storage, StorageError};
use wobblescope_core::eligibility::Coverage;
use wobblescope_core::fit::Axis;
use wobblescope_session::meta::{SegmentMeta, SessionMeta, Source};

const CAP: u64 = u64::MAX / 4;

fn initial(id: &str) -> SessionMeta {
    let mut meta = Spec::new(id, "2026-11-01T12:00:00Z")
        .chips(&["a", "b"])
        .meta();
    meta.segments.clear();
    meta
}

/// Writes segment `index` for both chips and appends it to `meta`.
fn segment(c: &mut wobblescope::storage::capture::Capture, meta: &mut SessionMeta, index: usize) {
    let (a, b) = (samples(300, 1.0), samples(200, 2.0));
    let files = c
        .write_segment(
            index,
            &[(0, &a, Coverage::Complete), (1, &b, Coverage::Complete)],
        )
        .unwrap();
    meta.segments.push(SegmentMeta {
        axis: if index == 0 { Axis::X } else { Axis::Y },
        point: 0,
        window: None,
        files,
    });
}

fn listed(s: &Storage) -> Vec<String> {
    let mut v: Vec<String> = s
        .list(&ListFilter::default())
        .into_iter()
        .map(|x| x.id)
        .collect();
    v.sort();
    v
}

fn leftovers(data: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(sessions_dir(data))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn admit_begin_write_two_segments_and_seal_lists_a_capture() {
    let tmp = TempDir::new().unwrap();
    let (storage, _) = open(tmp.path(), CAP);
    let reservation = storage.admit("test-1", 10_000).unwrap();
    assert_eq!(storage.usage().unwrap().reserved_bytes, 10_000);
    let mut meta = initial("test-1");
    let mut capture = storage.begin_capture(reservation, &meta).unwrap();
    segment(&mut capture, &mut meta, 0);
    capture.journal(&meta).unwrap();
    segment(&mut capture, &mut meta, 1);
    capture.journal(&meta).unwrap();
    // Not listed, not evictable and not deletable until sealed.
    assert!(listed(&storage).is_empty());
    assert!(matches!(
        storage.delete("test-1"),
        Err(StorageError::NotFound(_))
    ));

    let summary = storage.seal_capture(&capture, &meta).unwrap();
    drop(capture);
    assert_eq!(listed(&storage), ["test-1"]);
    assert_eq!(summary.outcome, OutcomeKind::Complete);
    let (summary, stored) = storage.session("test-1").unwrap();
    assert!(matches!(stored.source, Source::Capture));
    let usage = storage.usage().unwrap();
    assert_eq!(usage.reserved_bytes, 0);
    assert_eq!(usage.used_bytes, summary.size_bytes);
    assert_eq!(leftovers(tmp.path()), ["test-1"]);
}

#[test]
fn a_crashed_capture_is_recovered_on_open_listed_failed_and_reported() {
    let tmp = TempDir::new().unwrap();
    {
        let (storage, _) = open(tmp.path(), CAP);
        let reservation = storage.admit("test-2", 10_000).unwrap();
        let mut meta = initial("test-2");
        let mut capture = storage.begin_capture(reservation, &meta).unwrap();
        segment(&mut capture, &mut meta, 0);
        capture.journal(&meta).unwrap();
        // The daemon dies here: the capture is dropped without sealing.
        drop(capture);
    }
    let (storage, report) = open(tmp.path(), CAP);
    assert_eq!(listed(&storage), ["test-2"]);
    let summary = storage.get("test-2").unwrap();
    assert_eq!(summary.outcome, OutcomeKind::Failed);
    assert!(report.unfinished_capture_found());
    assert!(
        report
            .lines()
            .iter()
            .any(|l| l.contains("recovered unfinished capture test-2")),
        "{:?}",
        report.lines()
    );
}

#[test]
fn a_crash_before_segment_one_lists_nothing_removes_the_directory_and_still_reports_it() {
    let tmp = TempDir::new().unwrap();
    {
        let (storage, _) = open(tmp.path(), CAP);
        let reservation = storage.admit("test-3", 10_000).unwrap();
        let capture = storage
            .begin_capture(reservation, &initial("test-3"))
            .unwrap();
        drop(capture);
    }
    assert_eq!(leftovers(tmp.path()), [".test-3.tmp"]);
    let (storage, report) = open(tmp.path(), CAP);
    assert!(listed(&storage).is_empty());
    assert!(leftovers(tmp.path()).is_empty());
    assert!(report.unfinished_capture_found());
    assert!(report.lines().iter().any(|l| l.contains("discarded")));
}

#[test]
fn an_open_without_leftovers_reports_no_unfinished_capture() {
    let tmp = TempDir::new().unwrap();
    seal_in(tmp.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    let (_, report) = open(tmp.path(), CAP);
    assert!(!report.unfinished_capture_found());
}

#[test]
fn admit_refuses_an_id_whose_tmp_exists() {
    let tmp = TempDir::new().unwrap();
    let (storage, _) = open(tmp.path(), CAP);
    fs::create_dir(sessions_dir(tmp.path()).join(".test-4.tmp")).unwrap();
    assert!(matches!(
        storage.admit("test-4", 1),
        Err(StorageError::IdTaken(_))
    ));
}

#[test]
fn abandon_removes_the_tmp_and_frees_the_reservation_and_the_id() {
    let tmp = TempDir::new().unwrap();
    let (storage, _) = open(tmp.path(), CAP);
    let reservation = storage.admit("test-5", 10_000).unwrap();
    let capture = storage
        .begin_capture(reservation, &initial("test-5"))
        .unwrap();
    storage.abandon_capture(capture).unwrap();
    assert!(leftovers(tmp.path()).is_empty());
    assert_eq!(storage.usage().unwrap().reserved_bytes, 0);
    storage.admit("test-5", 10_000).unwrap();
}

#[test]
fn a_seal_with_a_bad_created_at_fails_leaves_the_tmp_and_abandon_removes_it() {
    let tmp = TempDir::new().unwrap();
    let (storage, _) = open(tmp.path(), CAP);
    let reservation = storage.admit("test-6", 10_000).unwrap();
    let mut meta = initial("test-6");
    let mut capture = storage.begin_capture(reservation, &meta).unwrap();
    segment(&mut capture, &mut meta, 0);
    meta.created_at = "yesterday".to_owned();
    assert!(matches!(
        storage.seal_capture(&capture, &meta),
        Err(StorageError::BadSession(_))
    ));
    assert_eq!(leftovers(tmp.path()), [".test-6.tmp"]);
    assert_eq!(storage.usage().unwrap().reserved_bytes, 10_000);
    storage.abandon_capture(capture).unwrap();
    assert!(leftovers(tmp.path()).is_empty());
}

#[test]
fn a_capture_is_never_evicted_by_another_admission() {
    let tmp = TempDir::new().unwrap();
    let (storage, _) = open(tmp.path(), 1_000_000);
    let reservation = storage.admit("test-7", 10_000).unwrap();
    let mut meta = initial("test-7");
    let mut capture = storage.begin_capture(reservation, &meta).unwrap();
    segment(&mut capture, &mut meta, 0);
    capture.journal(&meta).unwrap();
    // The cap cannot fit this one, and the capture in progress is not a candidate.
    assert!(matches!(
        storage.admit("other", 995_000),
        Err(StorageError::CapExceeded { .. })
    ));
    assert!(leftovers(tmp.path()).contains(&".test-7.tmp".to_owned()));
}

#[test]
fn ids_take_a_suffix_when_the_second_is_taken() {
    let base = base_id("test", "2026-11-01T12:14:00Z");
    assert_eq!(base, "test-20261101T121400Z");
    let mut tried = Vec::new();
    let found = first_free_id(&base, |id| {
        tried.push(id.to_owned());
        if tried.len() < 3 {
            Err(StorageError::IdTaken(id.to_owned()))
        } else {
            Ok(())
        }
    })
    .unwrap();
    assert_eq!(found.unwrap().0, format!("{base}-3"));
    assert_eq!(
        tried,
        [base.clone(), format!("{base}-2"), format!("{base}-3")]
    );
    let none = first_free_id(&base, |id| -> Result<(), _> {
        Err(StorageError::IdTaken(id.to_owned()))
    })
    .unwrap();
    assert!(none.is_none());
}

#[test]
fn a_capture_that_cannot_be_recovered_survives_opening_storage() {
    let tmp = TempDir::new().unwrap();
    {
        let (storage, _) = open(tmp.path(), CAP);
        let reservation = storage.admit("test-8", 10_000).unwrap();
        let mut meta = initial("test-8");
        let mut capture = storage.begin_capture(reservation, &meta).unwrap();
        segment(&mut capture, &mut meta, 0);
        capture.journal(&meta).unwrap();
    }
    // Something already stands where the recovered session would go.
    fs::create_dir(sessions_dir(tmp.path()).join("test-8")).unwrap();
    let (_storage, report) = open(tmp.path(), CAP);
    assert!(report.unfinished_capture_found());
    assert!(
        report
            .lines()
            .iter()
            .any(|l| l.contains("could not recover unfinished capture test-8")),
        "{:?}",
        report.lines()
    );
    let kept = sessions_dir(tmp.path()).join(".test-8.tmp");
    assert!(kept.join("meta.partial.json").exists());
    assert!(kept.join("c0-s0.zst").exists());
}
