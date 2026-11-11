mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use common::{Spec, probe, seal_in, sessions_dir, settings, source};
use tempfile::TempDir;
use time::OffsetDateTime;
use wobblescope::results::container::size_bound;
use wobblescope::results::record::{AxisSnapshot, SaveRequest, Snippets};
use wobblescope::storage::{Probe, Storage, StorageError};
use wobblescope_core::eligibility::Eligibility;
use wobblescope_core::fit::{Axis, FitParams};
use wobblescope_core::multichip::Analysis;

const CREATED: &str = "2026-10-01T10:00:00Z";
const HEADROOM: u64 = 1000;

fn analysis(core_version: &str) -> Analysis {
    Analysis {
        per_chip: vec![],
        worst_case: None,
        worst_case_vibrations: vec![],
        summed: None,
        failures: vec![],
        eligibility: Eligibility { decisions: vec![] },
        core_version: core_version.to_owned(),
    }
}

fn request(session_created_at: &str, core_version: &str) -> SaveRequest {
    SaveRequest {
        label: Some("before".to_owned()),
        core_version: core_version.to_owned(),
        session_created_at: session_created_at.to_owned(),
        fit_params: FitParams::default(),
        overrides: vec![],
        axes: vec![AxisSnapshot {
            axis: Axis::X,
            analysis: analysis(core_version),
            snippets: Snippets {
                worst_case: None,
                per_chip: vec![],
            },
        }],
    }
}

fn now() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap()
}

fn open(data: &Path, cap: u64, p: Probe) -> Storage {
    Storage::open(data, &settings(cap, HEADROOM), p).unwrap().0
}

fn plenty() -> Probe {
    probe(u64::MAX / 4)
}

fn with_session(cap: u64, p: Probe) -> (TempDir, Storage) {
    let dir = TempDir::new().unwrap();
    seal_in(
        dir.path(),
        &Spec::new("s1", CREATED).chips(&["a", "b"]).samples(300),
    );
    let storage = open(dir.path(), cap, p);
    (dir, storage)
}

fn results_dir(data: &Path) -> PathBuf {
    data.join("results")
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn save(storage: &Storage) -> String {
    storage
        .save_result("s1", request(CREATED, "0.1.0"), now())
        .unwrap()
        .id
}

#[test]
fn saves_a_result_beside_its_session() {
    let (dir, storage) = with_session(u64::MAX / 4, plenty());
    let summary = storage
        .save_result("s1", request(CREATED, "0.1.0"), now())
        .unwrap();
    assert!(
        results_dir(dir.path())
            .join(format!("{}.zip", summary.id))
            .is_file()
    );

    let list = storage.list_results().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].session_id.as_deref(), Some("s1"));
    assert!(list[0].unusable.is_none());

    let detail = storage.result(&summary.id).unwrap();
    assert_eq!(detail.meta, storage.session("s1").unwrap().1);
    assert_eq!(detail.record.core_version, "0.1.0");
    let on_disk = fs::read(sessions_dir(dir.path()).join("s1").join("c1-s0.zst")).unwrap();
    assert_eq!(storage.result_sample(&summary.id, 1, 0).unwrap(), on_disk);
    assert!(matches!(
        storage.result_sample(&summary.id, 5, 0),
        Err(StorageError::NoRecording { .. })
    ));
}

#[test]
fn refuses_a_save_when_the_session_was_replaced() {
    let (dir, storage) = with_session(u64::MAX / 4, plenty());
    storage.delete("s1").unwrap();
    let again = Spec::new("s1", "2026-10-02T10:00:00Z").chips(&["a", "b"]);
    let reservation = storage.admit("s1", 1000).unwrap();
    storage.store(reservation, &again.meta(), source).unwrap();

    let error = storage
        .save_result("s1", request(CREATED, "0.1.0"), now())
        .unwrap_err();
    assert!(
        matches!(error, StorageError::SessionChanged(_)),
        "{error:?}"
    );
    assert!(names(&results_dir(dir.path())).is_empty());
}

#[test]
fn refuses_a_save_that_would_eat_a_reservation() {
    let (dir, storage) = with_session(u64::MAX / 4, probe(100_000));
    let _held = storage.admit("big", 90_000).unwrap();
    let error = storage
        .save_result("s1", request(CREATED, "0.1.0"), now())
        .unwrap_err();
    assert!(
        matches!(error, StorageError::InsufficientDisk { .. }),
        "{error:?}"
    );
    assert!(names(&results_dir(dir.path())).is_empty());
}

#[test]
fn a_save_leases_the_session() {
    let (_dir, storage) = with_session(u64::MAX / 4, plenty());
    let seen = Arc::new(Mutex::new(None));
    let slot = seen.clone();
    storage.set_save_hook(move |s| {
        *slot.lock().unwrap() = Some(s.delete("s1"));
    });
    save(&storage);
    let outcome = seen.lock().unwrap().take().unwrap();
    assert!(
        matches!(outcome, Err(StorageError::InUse(_))),
        "{outcome:?}"
    );
    storage.delete("s1").unwrap();
}

#[test]
fn survives_session_delete_and_eviction() {
    let (_dir, storage) = with_session(u64::MAX / 4, plenty());
    let id = save(&storage);
    storage.delete("s1").unwrap();
    assert_eq!(storage.result(&id).unwrap().record.session_id, "s1");
    assert!(!storage.result_sample(&id, 0, 0).unwrap().is_empty());
}

fn flip(path: &Path, at: usize) {
    let mut bytes = fs::read(path).unwrap();
    bytes[at] ^= 0xFF;
    fs::write(path, bytes).unwrap();
}

#[test]
fn lists_a_damaged_file_as_unusable() {
    let (dir, storage) = with_session(u64::MAX / 4, plenty());
    let good = save(&storage);
    let later = now() + time::Duration::seconds(60);
    let bad = storage
        .save_result("s1", request(CREATED, "0.1.0"), later)
        .unwrap()
        .id;
    let path = results_dir(dir.path()).join(format!("{bad}.zip"));

    // Offset 100 lies in the first entry, meta.json. A flipped byte there fails the manifest.
    // The result is listed as unusable with a reason and the others stay intact.
    flip(&path, 100);
    let list = storage.list_results().unwrap();
    assert_eq!(list.len(), 2);
    assert!(list[0].unusable.is_none());
    assert_eq!(list[0].id, good);
    assert_eq!(list[1].id, bad);
    assert!(list[1].unusable.as_deref().is_some_and(|r| !r.is_empty()));
    assert!(matches!(
        storage.result(&bad),
        Err(StorageError::ResultUnusable { .. })
    ));
    assert!(matches!(
        storage.result_sample(&bad, 0, 0),
        Err(StorageError::ResultUnusable { .. })
    ));
    storage.result(&good).unwrap();

    // Damage to result.json makes the file unusable in the list too, and it sorts last.
    flip(&path, 100);
    let bytes = fs::read(&path).unwrap();
    let at = bytes
        .windows(12)
        .rposition(|w| w == b"\"session_id\"")
        .unwrap();
    flip(&path, at);
    let list = storage.list_results().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, good);
    assert_eq!(list[1].id, bad);
    assert!(list[1].unusable.is_some());
}

#[test]
fn lists_newest_first() {
    let (_dir, storage) = with_session(u64::MAX / 4, plenty());
    let old = save(&storage);
    let new = storage
        .save_result(
            "s1",
            request(CREATED, "0.1.0"),
            now() + time::Duration::seconds(5),
        )
        .unwrap()
        .id;
    let ids: Vec<String> = storage
        .list_results()
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(ids, [new, old]);
}

#[test]
fn startup_sweeps_a_temporary_result() {
    let (dir, storage) = with_session(u64::MAX / 4, plenty());
    drop(storage);
    let leftover = results_dir(dir.path()).join(".r1-000000.zip.tmp");
    fs::write(&leftover, b"partial").unwrap();
    let (_storage, report) =
        Storage::open(dir.path(), &settings(u64::MAX / 4, HEADROOM), plenty()).unwrap();
    assert!(!leftover.exists());
    assert!(
        report
            .swept
            .iter()
            .any(|n| n.ends_with(".r1-000000.zip.tmp"))
    );
}

#[test]
fn rejects_a_mismatched_core_version() {
    let (dir, storage) = with_session(u64::MAX / 4, plenty());
    let mut bad = request(CREATED, "0.1.0");
    bad.axes[0].analysis.core_version = "0.2.0".to_owned();
    let error = storage.save_result("s1", bad, now()).unwrap_err();
    assert!(matches!(error, StorageError::BadRequest(_)), "{error:?}");
    assert!(names(&results_dir(dir.path())).is_empty());
}

#[test]
fn usage_counts_results_apart_from_sessions() {
    let (_dir, storage) = with_session(u64::MAX / 4, plenty());
    let before = storage.usage().unwrap();
    assert_eq!((before.result_count, before.result_bytes), (0, 0));
    let id = save(&storage);
    let after = storage.usage().unwrap();
    assert_eq!(after.result_count, 1);
    assert_eq!(
        after.result_bytes,
        storage.list_results().unwrap()[0].size_bytes
    );
    assert_eq!(after.used_bytes, before.used_bytes);
    assert_eq!(after.reserved_bytes, 0);
    assert_eq!(after.session_count, 1);
    storage.delete_result(&id).unwrap();
    assert_eq!(storage.usage().unwrap().result_count, 0);
    assert!(matches!(
        storage.delete_result(&id),
        Err(StorageError::ResultNotFound(_))
    ));
}

#[test]
fn a_failed_write_leaves_no_temporary_file() {
    let free = Arc::new(AtomicU64::new(u64::MAX / 4));
    let reading = free.clone();
    let p: Probe = Arc::new(move |_| Ok(reading.load(Ordering::SeqCst)));
    let (dir, storage) = with_session(u64::MAX / 4, p);
    let sample = sessions_dir(dir.path()).join("s1").join("c0-s0.zst");
    let bytes = fs::read(&sample).unwrap();

    // Room for exactly one save, so a second one succeeds only if the first released its space.
    let need = bound_for(dir.path(), &storage) + HEADROOM;
    free.store(need, Ordering::SeqCst);
    let doomed = sample.clone();
    storage.set_save_hook(move |_| fs::remove_file(&doomed).unwrap());
    let error = storage
        .save_result("s1", request(CREATED, "0.1.0"), now())
        .unwrap_err();
    assert!(matches!(error, StorageError::Io(_)), "{error:?}");
    assert!(names(&results_dir(dir.path())).is_empty());

    fs::write(&sample, bytes).unwrap();
    storage.set_save_hook(|_| {});
    save(&storage);
}

#[test]
fn a_result_reservation_does_not_trigger_eviction() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-02T10:00:00Z"));
    seal_in(dir.path(), &Spec::new("s0", "2026-10-01T10:00:00Z"));
    let probe_storage = open(dir.path(), u64::MAX / 4, plenty());
    let used = probe_storage.usage().unwrap().used_bytes;
    drop(probe_storage);
    let storage = open(dir.path(), used + 1000, plenty());

    let outcome = Arc::new(Mutex::new(None));
    let slot = outcome.clone();
    storage.set_save_hook(move |s| {
        *slot.lock().unwrap() = Some(s.admit("new", 1000).map(|_| ()));
    });
    storage
        .save_result("s1", request(CREATED_S1, "0.1.0"), now())
        .unwrap();
    assert!(outcome.lock().unwrap().take().unwrap().is_ok());
    assert!(storage.get("s0").is_some(), "s0 was evicted");
}

const CREATED_S1: &str = "2026-10-02T10:00:00Z";

/// The `size_bound` a save of `s1` computes, rebuilt from a finished result. Needs free space
/// to be plentiful when called.
fn bound_for(data: &Path, storage: &Storage) -> u64 {
    let id = save(storage);
    let detail = storage.result(&id).unwrap();
    storage.delete_result(&id).unwrap();
    let mut record = detail.record;
    record.manifest.clear();
    let json_len = serde_json::to_vec(&record).unwrap().len();
    let meta_len = detail.meta.to_json().unwrap().len() as u64;
    let sizes: Vec<u64> = ["c0-s0.zst", "c1-s0.zst"]
        .iter()
        .map(|n| {
            fs::metadata(sessions_dir(data).join("s1").join(n))
                .unwrap()
                .len()
        })
        .collect();
    size_bound(json_len, meta_len, &sizes)
}

#[test]
fn admission_counts_the_container_overhead() {
    let free = Arc::new(AtomicU64::new(u64::MAX / 4));
    let reading = free.clone();
    let p: Probe = Arc::new(move |_| Ok(reading.load(Ordering::SeqCst)));
    let (dir, storage) = with_session(u64::MAX / 4, p);

    // The id has the same length on every save, so the record sizes alike.
    let expected = bound_for(dir.path(), &storage) + HEADROOM;
    free.store(expected - 1, Ordering::SeqCst);
    let error = storage
        .save_result("s1", request(CREATED, "0.1.0"), now())
        .unwrap_err();
    assert!(
        matches!(error, StorageError::InsufficientDisk { need, .. } if need == expected),
        "{error:?}"
    );
    free.store(expected, Ordering::SeqCst);
    let summary = save_summary(&storage);
    assert!(summary.size_bytes <= expected - HEADROOM);
    let on_disk = fs::metadata(results_dir(dir.path()).join(format!("{}.zip", summary.id)))
        .unwrap()
        .len();
    assert_eq!(on_disk, summary.size_bytes);
}

fn save_summary(storage: &Storage) -> wobblescope::results::record::ResultSummary {
    storage
        .save_result("s1", request(CREATED, "0.1.0"), now())
        .unwrap()
}

#[test]
fn stores_the_session_meta_byte_for_byte() {
    let (dir, storage) = with_session(u64::MAX / 4, plenty());
    let path = sessions_dir(dir.path()).join("s1").join("meta.json");
    let text = fs::read_to_string(&path).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let odd = format!("{}\n\n", serde_json::to_string_pretty(&value).unwrap());
    assert_ne!(odd, text);
    fs::write(&path, &odd).unwrap();

    let id = save(&storage);
    let zip = results_dir(dir.path()).join(format!("{id}.zip"));
    let file = wobblescope::results::container::ResultFile::open(&zip).unwrap();
    file.verify().unwrap();
    let stored = {
        let mut archive = zip::ZipArchive::new(fs::File::open(&zip).unwrap()).unwrap();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut archive.by_name("meta.json").unwrap(), &mut bytes).unwrap();
        bytes
    };
    assert_eq!(stored, odd.as_bytes());
}
