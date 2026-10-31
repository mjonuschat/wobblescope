mod common;

use std::fs;
use std::path::Path;

use common::{Spec, probe, seal_in, sessions_dir, settings, size_of, source};
use tempfile::TempDir;
use wobblescope::storage::index::{ListFilter, OutcomeKind};
use wobblescope::storage::{Probe, Storage, StorageError};
use wobblescope_session::SessionError;
use wobblescope_session::meta::{Outcome, Source};

/// Four sessions of identical size, s1 the oldest.
fn four(data: &Path) {
    for (i, day) in ["01", "02", "03", "04"].iter().enumerate() {
        seal_in(
            data,
            &Spec::new(&format!("s{}", i + 1), &format!("2026-10-{day}T10:00:00Z")),
        );
    }
}

fn open_with(data: &Path, cap: u64, headroom: u64, p: Probe) -> Storage {
    Storage::open(data, &settings(cap, headroom), p).unwrap().0
}

/// Total bytes of the four-session fixture, and the size of one.
fn totals(data: &Path) -> (u64, u64) {
    let s = open_with(data, u64::MAX / 4, 0, probe(u64::MAX / 4));
    let one = size_of(&s, "s1");
    (s.usage().unwrap().used_bytes, one)
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

fn on_disk(data: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(sessions_dir(data))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn ae1_the_oldest_unpinned_session_goes_first() {
    let dir = TempDir::new().unwrap();
    four(dir.path());
    let (total, one) = totals(dir.path());
    let s = open_with(dir.path(), total, 0, probe(u64::MAX / 4));
    s.pin("s1").unwrap();
    let r = s.admit("new", one).unwrap();
    assert_eq!(ids(&s), ["s1", "s3", "s4"]);
    assert_eq!(s.usage().unwrap().reserved_bytes, one);
    drop(r);
}

#[test]
fn an_estimate_just_over_one_session_evicts_two() {
    let dir = TempDir::new().unwrap();
    four(dir.path());
    let (total, one) = totals(dir.path());
    let s = open_with(dir.path(), total, 0, probe(u64::MAX / 4));
    let _r = s.admit("new", one + 1).unwrap();
    assert_eq!(ids(&s), ["s3", "s4"]);
}

#[test]
fn a_leased_session_is_skipped() {
    let dir = TempDir::new().unwrap();
    four(dir.path());
    let (total, one) = totals(dir.path());
    let s = open_with(dir.path(), total, 0, probe(u64::MAX / 4));
    let _lease = s.lease("s1").unwrap();
    let _r = s.admit("new", one).unwrap();
    assert_eq!(ids(&s), ["s1", "s3", "s4"]);
}

#[test]
fn ae2_with_everything_pinned_nothing_is_deleted() {
    let dir = TempDir::new().unwrap();
    four(dir.path());
    let (total, _) = totals(dir.path());
    let s = open_with(dir.path(), total, 0, probe(u64::MAX / 4));
    for id in ["s1", "s2", "s3", "s4"] {
        s.pin(id).unwrap();
    }
    match s.admit("new", 1) {
        Err(StorageError::CapExceeded {
            need,
            cap,
            pinned_bytes,
            evictable_bytes,
        }) => {
            assert_eq!(
                (need, cap, pinned_bytes, evictable_bytes),
                (1, total, total, 0)
            );
        }
        other => panic!("expected CapExceeded, got {:?}", other.err()),
    }
    assert_eq!(on_disk(dir.path()), ["s1", "s2", "s3", "s4"]);
}

#[test]
fn an_estimate_larger_than_the_cap_is_refused_on_an_empty_store() {
    let dir = TempDir::new().unwrap();
    let s = open_with(dir.path(), 1000, 0, probe(u64::MAX / 4));
    assert!(matches!(
        s.admit("new", 1001),
        Err(StorageError::CapExceeded { .. })
    ));
}

#[test]
fn ae3_low_disk_under_the_cap_refuses_without_deleting() {
    let dir = TempDir::new().unwrap();
    four(dir.path());
    let (total, one) = totals(dir.path());
    let s = open_with(dir.path(), total * 10, 1000, probe(one + 999));
    match s.admit("new", one) {
        Err(StorageError::InsufficientDisk { need, available }) => {
            assert_eq!((need, available), (one + 1000, one + 999));
        }
        other => panic!("expected InsufficientDisk, got {:?}", other.err()),
    }
    assert_eq!(on_disk(dir.path()).len(), 4);
}

#[test]
fn eviction_counts_toward_free_space_but_one_byte_short_deletes_nothing() {
    let dir = TempDir::new().unwrap();
    four(dir.path());
    let (total, one) = totals(dir.path());
    let headroom = 500;
    // Evicting one session frees `one` bytes, so the probe may report that much less.
    let enough = probe(one + headroom - one);
    let s = open_with(dir.path(), total, headroom, enough);
    let _r = s.admit("new", one).unwrap();
    assert_eq!(ids(&s), ["s2", "s3", "s4"]);

    let dir = TempDir::new().unwrap();
    four(dir.path());
    let short = probe(one + headroom - one - 1);
    let s = open_with(dir.path(), total, headroom, short);
    assert!(matches!(
        s.admit("new", one),
        Err(StorageError::InsufficientDisk { .. })
    ));
    assert_eq!(on_disk(dir.path()).len(), 4);
}

#[test]
fn a_probe_error_refuses_the_admission_and_deletes_nothing() {
    let dir = TempDir::new().unwrap();
    four(dir.path());
    let (total, one) = totals(dir.path());
    let broken: Probe = std::sync::Arc::new(|_| Err(std::io::Error::other("statvfs failed")));
    let s = open_with(dir.path(), total, 0, broken);
    assert!(matches!(s.admit("new", one), Err(StorageError::Io(_))));
    assert_eq!(on_disk(dir.path()).len(), 4);
}

#[test]
fn live_reservations_count_against_free_space_too() {
    let dir = TempDir::new().unwrap();
    let s = open_with(dir.path(), u64::MAX / 4, 100, probe(1099));
    let first = s.admit("a", 500).unwrap();
    // 500 bytes are promised to `first`, so 599 remain against a need of 700.
    assert!(matches!(
        s.admit("b", 600),
        Err(StorageError::InsufficientDisk { .. })
    ));
    drop(first);
    let _second = s.admit("b", 600).unwrap();
}

#[test]
fn eviction_bytes_are_added_before_reserved_bytes_are_subtracted() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    let dir = TempDir::new().unwrap();
    four(dir.path());
    let (total, one) = totals(dir.path());
    let free = Arc::new(AtomicU64::new(u64::MAX / 4));
    let reading = free.clone();
    let p: Probe = Arc::new(move |_| Ok(reading.load(Ordering::SeqCst)));
    let s = open_with(dir.path(), total + 400, 0, p);
    let _held = s.admit("held", 400).unwrap();
    // The cap is full, so admitting `one` evicts one session. After that, 300 free bytes
    // plus the evicted `one`, minus the 400 promised to `held`, falls short of `one`.
    free.store(300, Ordering::SeqCst);
    assert!(matches!(
        s.admit("new", one),
        Err(StorageError::InsufficientDisk { .. })
    ));
    assert_eq!(on_disk(dir.path()).len(), 4);
}

#[test]
fn ids_that_exist_or_are_reserved_or_invalid_are_refused() {
    let dir = TempDir::new().unwrap();
    four(dir.path());
    fs::create_dir(sessions_dir(dir.path()).join("odd")).unwrap();
    fs::write(
        sessions_dir(dir.path()).join("odd").join("meta.json"),
        "{bad",
    )
    .unwrap();
    let s = open_with(dir.path(), u64::MAX / 4, 0, probe(u64::MAX / 4));
    assert!(matches!(s.admit("s1", 1), Err(StorageError::IdTaken(_))));
    assert!(matches!(s.admit("odd", 1), Err(StorageError::IdTaken(_))));
    let held = s.admit("fresh", 1).unwrap();
    assert!(matches!(s.admit("fresh", 1), Err(StorageError::IdTaken(_))));
    drop(held);
    assert!(s.admit("fresh", 1).is_ok());
    assert!(matches!(
        s.admit("a b", 1),
        Err(StorageError::Session(SessionError::Invalid { .. }))
    ));
}

#[test]
fn reserved_bytes_count_toward_the_next_admission_and_clear_on_drop() {
    let dir = TempDir::new().unwrap();
    let s = open_with(dir.path(), 1000, 0, probe(u64::MAX / 4));
    let r = s.admit("a", 700).unwrap();
    assert_eq!(s.usage().unwrap().reserved_bytes, 700);
    assert!(matches!(
        s.admit("b", 400),
        Err(StorageError::CapExceeded { .. })
    ));
    drop(r);
    assert_eq!(s.usage().unwrap().reserved_bytes, 0);
    assert!(s.admit("b", 400).is_ok());
}

#[test]
fn ae5_a_lower_cap_deletes_nothing_until_the_next_admission() {
    let dir = TempDir::new().unwrap();
    four(dir.path());
    let (total, one) = totals(dir.path());
    let s = open_with(dir.path(), one * 2, 0, probe(u64::MAX / 4));
    assert_eq!(on_disk(dir.path()).len(), 4);
    assert_eq!(s.usage().unwrap().used_bytes, total);
    let _r = s.admit("new", one).unwrap();
    assert_eq!(ids(&s), ["s4"]);
}

#[test]
fn ae4_a_newer_format_session_survives_every_admission() {
    let dir = TempDir::new().unwrap();
    four(dir.path());
    seal_in(dir.path(), &Spec::new("newer", "2020-01-01T00:00:00Z"));
    let p = sessions_dir(dir.path()).join("newer").join("meta.json");
    let text = fs::read_to_string(&p)
        .unwrap()
        .replace("\"format_version\": 1", "\"format_version\": 2");
    fs::write(&p, text).unwrap();
    let (_, one) = totals(dir.path());
    let s = open_with(dir.path(), one * 4, 0, probe(u64::MAX / 4));
    for i in 0..3 {
        let _r = s.admit(&format!("n{i}"), one).unwrap();
    }
    assert!(p.exists());
}

#[test]
fn imports_and_failed_captures_are_evicted_before_a_newer_completed_one() {
    let dir = TempDir::new().unwrap();
    let mut imported = Spec::new("s1", "2026-10-01T10:00:00Z");
    imported.source = Source::CsvImport {
        original_file_name: None,
    };
    let mut failed = Spec::new("s2", "2026-10-02T10:00:00Z");
    failed.outcome = Outcome::Failed {
        reason: "x".to_owned(),
    };
    seal_in(dir.path(), &imported);
    seal_in(dir.path(), &failed);
    seal_in(dir.path(), &Spec::new("s3", "2026-10-03T10:00:00Z"));
    let big = open_with(dir.path(), u64::MAX / 4, 0, probe(u64::MAX / 4));
    let total = big.usage().unwrap().used_bytes;
    let two_oldest = size_of(&big, "s1") + size_of(&big, "s2");
    drop(big);
    let s = open_with(dir.path(), total, 0, probe(u64::MAX / 4));
    let _r = s.admit("new", two_oldest).unwrap();
    assert_eq!(ids(&s), ["s3"]);
}

#[cfg(unix)]
#[test]
fn an_eviction_that_cannot_delete_returns_the_error_and_keeps_the_listing() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    four(dir.path());
    let (total, one) = totals(dir.path());
    let s = open_with(dir.path(), total, 0, probe(u64::MAX / 4));
    let sd = sessions_dir(dir.path());
    fs::set_permissions(&sd, fs::Permissions::from_mode(0o555)).unwrap();
    let result = s.admit("new", one);
    fs::set_permissions(&sd, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(result, Err(StorageError::Session(_))));
    assert_eq!(ids(&s).len(), 4);
}

#[cfg(unix)]
#[test]
fn a_delete_that_fails_after_the_rename_forgets_the_session_and_the_next_start_sweeps_it() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    four(dir.path());
    let (total, one) = totals(dir.path());
    let s = open_with(dir.path(), total, 0, probe(u64::MAX / 4));
    // The victim's own directory is read-only: renaming it succeeds, emptying it does not.
    let victim = sessions_dir(dir.path()).join("s1");
    fs::set_permissions(&victim, fs::Permissions::from_mode(0o555)).unwrap();
    let result = s.admit("new", one);
    let tombstone = sessions_dir(dir.path()).join(".s1.deleting");
    fs::set_permissions(&tombstone, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(result, Err(StorageError::Session(_))));
    assert_eq!(ids(&s), ["s2", "s3", "s4"]);
    assert_eq!(s.usage().unwrap().reserved_bytes, 0);
    assert!(tombstone.exists());
    let (_, report) = Storage::open(dir.path(), &settings(total, 0), probe(u64::MAX / 4)).unwrap();
    assert_eq!(report.swept, [".s1.deleting"]);
    assert!(!tombstone.exists());
}

#[test]
fn store_lists_the_session_at_once_with_its_real_size() {
    let dir = TempDir::new().unwrap();
    let s = open_with(dir.path(), u64::MAX / 4, 0, probe(u64::MAX / 4));
    let spec = Spec::new("cap1", "2026-10-05T10:00:00Z");
    let r = s.admit("cap1", 10_000).unwrap();
    let summary = s.store(r, &spec.meta(), source).unwrap();
    let on_disk_size = wobblescope_session::fs::open_session(&sessions_dir(dir.path()), "cap1")
        .unwrap()
        .size_bytes()
        .unwrap();
    assert_eq!(summary.size_bytes, on_disk_size);
    assert_eq!(s.get("cap1").unwrap().size_bytes, on_disk_size);
    assert_eq!(s.usage().unwrap().reserved_bytes, 0);
}

#[test]
fn store_refuses_a_mismatched_id_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let s = open_with(dir.path(), u64::MAX / 4, 0, probe(u64::MAX / 4));
    let r = s.admit("one", 100).unwrap();
    let spec = Spec::new("two", "2026-10-05T10:00:00Z");
    assert!(s.store(r, &spec.meta(), source).is_err());
    assert!(on_disk(dir.path()).is_empty());
    assert_eq!(s.usage().unwrap().reserved_bytes, 0);
}

#[test]
fn store_refuses_an_unparseable_created_at_before_writing() {
    let dir = TempDir::new().unwrap();
    let s = open_with(dir.path(), u64::MAX / 4, 0, probe(u64::MAX / 4));
    let r = s.admit("soon", 100).unwrap();
    let spec = Spec::new("soon", "soon");
    assert!(s.store(r, &spec.meta(), source).is_err());
    assert!(on_disk(dir.path()).is_empty());
}

#[test]
fn a_failing_source_releases_the_reservation_and_leaves_no_directory() {
    let dir = TempDir::new().unwrap();
    let s = open_with(dir.path(), u64::MAX / 4, 0, probe(u64::MAX / 4));
    let spec = Spec::new("bad", "2026-10-05T10:00:00Z").chips(&["a", "b"]);
    let r = s.admit("bad", 100).unwrap();
    let mut calls = 0;
    let result = s.store(r, &spec.meta(), |f| {
        calls += 1;
        if calls == 2 {
            Err(SessionError::Truncated)
        } else {
            source(f)
        }
    });
    assert!(result.is_err());
    assert!(on_disk(dir.path()).is_empty());
    assert_eq!(s.usage().unwrap().reserved_bytes, 0);
    assert!(s.admit("bad", 100).is_ok());
}

#[test]
fn a_failed_capture_is_stored_like_any_other() {
    let dir = TempDir::new().unwrap();
    let s = open_with(dir.path(), u64::MAX / 4, 0, probe(u64::MAX / 4));
    let mut spec = Spec::new("fail1", "2026-10-05T10:00:00Z");
    spec.outcome = Outcome::Failed {
        reason: "klipper shutdown".to_owned(),
    };
    let r = s.admit("fail1", 100).unwrap();
    s.store(r, &spec.meta(), source).unwrap();
    assert_eq!(s.get("fail1").unwrap().outcome, OutcomeKind::Failed);
}
