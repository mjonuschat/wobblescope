mod common;

use std::fs;

use common::{Spec, open, seal_in, sessions_dir, settings, size_of};
use tempfile::TempDir;
use wobblescope::storage::index::ListFilter;
use wobblescope::storage::pins::Pins;
use wobblescope::storage::{Storage, StorageError};

const BIG: u64 = u64::MAX / 4;

fn ids(storage: &Storage) -> Vec<String> {
    storage
        .list(&ListFilter::default())
        .into_iter()
        .map(|s| s.id)
        .collect()
}

#[test]
fn storage_is_a_cloneable_thread_safe_handle() {
    fn check<T: Clone + Send + Sync + 'static>() {}
    check::<Storage>();
}

#[test]
fn opening_a_missing_directory_creates_it_and_reports_nothing() {
    let dir = TempDir::new().unwrap();
    let data = dir.path().join("fresh");
    let (_s, report) = open(&data, BIG);
    assert!(sessions_dir(&data).is_dir());
    assert!(report.skipped.is_empty() && report.swept.is_empty() && report.pruned_pins.is_empty());
}

#[test]
fn opening_sweeps_crash_leftovers_and_reports_them() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    fs::create_dir(sessions_dir(dir.path()).join(".x.tmp")).unwrap();
    fs::create_dir(sessions_dir(dir.path()).join(".y.deleting")).unwrap();
    let (s, report) = open(dir.path(), BIG);
    let mut swept = report.swept.clone();
    swept.sort();
    assert_eq!(swept, [".x.tmp", ".y.deleting"]);
    assert_eq!(ids(&s), ["s1"]);
    assert!(!sessions_dir(dir.path()).join(".x.tmp").exists());
}

#[test]
fn pins_for_missing_sessions_are_pruned_and_pins_for_skipped_ones_stay() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    seal_in(dir.path(), &Spec::new("newer", "2026-10-01T10:00:00Z"));
    let p = sessions_dir(dir.path()).join("newer").join("meta.json");
    let text = fs::read_to_string(&p)
        .unwrap()
        .replace("\"format_version\": 1", "\"format_version\": 2");
    fs::write(&p, text).unwrap();
    fs::write(
        dir.path().join("pins.json"),
        r#"{"version":1,"pinned":["gone","newer"]}"#,
    )
    .unwrap();
    let (s, report) = open(dir.path(), BIG);
    assert_eq!(report.pruned_pins, ["gone"]);
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].name, "newer");
    s.pin("s1").unwrap();
    let saved = Pins::load(&dir.path().join("pins.json")).unwrap();
    assert_eq!(saved.iter().collect::<Vec<_>>(), ["newer", "s1"]);
}

#[test]
fn a_corrupt_pins_file_stops_open_and_deletes_nothing() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    fs::write(dir.path().join("pins.json"), "garbage").unwrap();
    let err = Storage::open(dir.path(), &settings(BIG, 0), common::probe(BIG))
        .err()
        .expect("open must fail");
    assert!(err.to_string().contains("pins.json"), "{err}");
    assert!(
        sessions_dir(dir.path())
            .join("s1")
            .join("meta.json")
            .exists()
    );
}

#[test]
fn pins_survive_a_reopen_and_unpinning_clears_them() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    s.pin("s1").unwrap();
    s.pin("s1").unwrap();
    assert!(s.get("s1").unwrap().pinned);
    let (s, _) = open(dir.path(), BIG);
    assert!(s.get("s1").unwrap().pinned);
    s.unpin("s1").unwrap();
    let (s, _) = open(dir.path(), BIG);
    assert!(!s.get("s1").unwrap().pinned);
}

#[test]
fn pinning_an_unknown_id_is_not_found() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    assert!(matches!(s.pin("nope"), Err(StorageError::NotFound(_))));
}

#[cfg(unix)]
#[test]
fn a_failed_pin_write_leaves_the_session_unpinned() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
    let result = s.pin("s1");
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    assert!(!s.get("s1").unwrap().pinned);
}

#[test]
fn deleting_an_unpinned_session_removes_it_from_the_listing_and_from_disk() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    s.delete("s1").unwrap();
    assert!(ids(&s).is_empty());
    assert!(!sessions_dir(dir.path()).join("s1").exists());
}

#[test]
fn delete_refuses_pinned_leased_and_unknown_sessions() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    seal_in(dir.path(), &Spec::new("s2", "2026-10-02T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    s.pin("s1").unwrap();
    assert!(matches!(s.delete("s1"), Err(StorageError::Pinned(_))));
    assert!(
        sessions_dir(dir.path())
            .join("s1")
            .join("meta.json")
            .exists()
    );
    let lease = s.lease("s2").unwrap();
    assert!(matches!(s.delete("s2"), Err(StorageError::InUse(_))));
    drop(lease);
    s.delete("s2").unwrap();
    assert!(matches!(s.delete("s2"), Err(StorageError::NotFound(_))));
}

#[test]
fn leases_count_and_need_a_known_session() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    assert!(matches!(s.lease("nope"), Err(StorageError::NotFound(_))));
    let a = s.lease("s1").unwrap();
    let b = s.lease("s1").unwrap();
    drop(a);
    assert!(matches!(s.delete("s1"), Err(StorageError::InUse(_))));
    drop(b);
    s.delete("s1").unwrap();
}

#[test]
fn a_clone_sees_changes_made_through_another_thread() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    let other = s.clone();
    std::thread::spawn(move || other.pin("s1").unwrap())
        .join()
        .unwrap();
    assert!(s.get("s1").unwrap().pinned);
}

#[test]
fn usage_reports_every_number() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    seal_in(dir.path(), &Spec::new("s2", "2026-10-02T10:00:00Z"));
    let (s, _) = Storage::open(dir.path(), &settings(1_000_000, 0), common::probe(777)).unwrap();
    s.pin("s1").unwrap();
    let u = s.usage().unwrap();
    assert_eq!(u.used_bytes, size_of(&s, "s1") + size_of(&s, "s2"));
    assert_eq!(u.pinned_bytes, size_of(&s, "s1"));
    assert_eq!(u.reserved_bytes, 0);
    assert_eq!(u.cap_bytes, 1_000_000);
    assert_eq!(u.free_bytes, 777);
    assert_eq!(u.session_count, 2);
}
