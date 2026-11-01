#![cfg(all(feature = "fs", not(target_arch = "wasm32")))]

use std::fs;
use std::path::Path;

use tempfile::TempDir;
use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::{Coverage, Health};
use wobblescope_core::fit::Axis;
use wobblescope_session::SessionError;
use wobblescope_session::fs::{list_sealed, open_session, sweep_leftovers};
use wobblescope_session::meta::{
    ChipMeta, Outcome, ProbePoint, SegmentMeta, SessionMeta, Source, TimeDomain,
};
use wobblescope_session::writer::{
    INTERRUPTED_REASON, JOURNAL_FILE, Recovery, SessionWriter, recover_unsealed,
};

fn samples(n: usize, seed: f64) -> RawSamples {
    let mut s = RawSamples::default();
    for i in 0..n {
        s.t.push(100.0 + i as f64 * 0.0003125);
        s.x.push((i as f64 * 0.37 + seed).sin() * 300.0);
        s.y.push(seed);
        s.z.push(-9806.0);
    }
    s
}

fn base(id: &str) -> SessionMeta {
    let chip = |name: &str| ChipMeta {
        name: name.to_owned(),
        chip_type: None,
        nominal_rate_hz: None,
        observed_rate_hz: None,
        axes_map: None,
        mounting: None,
        health: Health::Ok,
        health_notes: vec![],
    };
    SessionMeta {
        format_version: 1,
        id: id.to_owned(),
        created_at: "2026-11-01T12:00:00Z".to_owned(),
        source: Source::Capture,
        time_domain: TimeDomain::KlipperPrintTime,
        mode: None,
        settings: None,
        printer: None,
        outcome: Outcome::Complete,
        notes: vec![],
        probe_points: vec![ProbePoint { position: None }],
        chips: vec![chip("a"), chip("b")],
        segments: vec![],
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// Writes segment `index` for both chips and appends it to `meta`.
fn add_segment(writer: &mut SessionWriter, meta: &mut SessionMeta, index: usize, axis: Axis) {
    let a = samples(400, 1.0 + index as f64);
    let b = samples(300, 2.0 + index as f64);
    let files = writer
        .write_segment(
            index,
            &[(0, &a, Coverage::Complete), (1, &b, Coverage::Complete)],
        )
        .unwrap();
    meta.segments.push(SegmentMeta {
        axis,
        point: 0,
        window: None,
        files,
    });
}

#[test]
fn create_write_two_segments_journal_and_seal_reads_back() {
    let root = TempDir::new().unwrap();
    let mut meta = base("t1");
    let mut w = SessionWriter::create(root.path(), "t1", &meta).unwrap();
    assert_eq!(names(root.path()), [".t1.tmp"]);
    assert_eq!(names(&root.path().join(".t1.tmp")), [JOURNAL_FILE]);

    add_segment(&mut w, &mut meta, 0, Axis::X);
    w.journal(&meta).unwrap();
    add_segment(&mut w, &mut meta, 1, Axis::Y);
    w.journal(&meta).unwrap();
    w.seal(&meta).unwrap();

    assert_eq!(list_sealed(root.path()).unwrap(), ["t1"]);
    let session = open_session(root.path(), "t1").unwrap();
    assert_eq!(session.meta(), &meta);
    let file = &meta.segments[1].files[1];
    assert_eq!(session.load(file).unwrap(), samples(300, 3.0));
    assert_eq!(
        names(&root.path().join("t1")),
        [
            "c0-s0.zst",
            "c0-s1.zst",
            "c1-s0.zst",
            "c1-s1.zst",
            "meta.json"
        ]
    );
}

#[test]
fn the_journal_after_segment_one_recovers_to_a_failed_session_with_one_segment() {
    let root = TempDir::new().unwrap();
    let mut meta = base("t2");
    let mut w = SessionWriter::create(root.path(), "t2", &meta).unwrap();
    add_segment(&mut w, &mut meta, 0, Axis::X);
    w.journal(&meta).unwrap();
    // Segment 2's files reach the disk but the daemon dies before its journal.
    let mut later = meta.clone();
    add_segment(&mut w, &mut later, 1, Axis::Y);
    drop(w);

    let found = recover_unsealed(root.path()).unwrap();
    assert_eq!(
        found,
        [Recovery::Failed {
            id: "t2".to_owned(),
            segments: 1
        }]
    );
    let session = open_session(root.path(), "t2").unwrap();
    assert_eq!(session.meta().segments.len(), 1);
    assert_eq!(
        session.meta().outcome,
        Outcome::Failed {
            reason: INTERRUPTED_REASON.to_owned()
        }
    );
    assert_eq!(
        names(&root.path().join("t2")),
        ["c0-s0.zst", "c1-s0.zst", "meta.json"]
    );
}

#[test]
fn a_journal_naming_a_segment_whose_file_is_missing_recovers_without_it() {
    let root = TempDir::new().unwrap();
    let mut meta = base("t3");
    let mut w = SessionWriter::create(root.path(), "t3", &meta).unwrap();
    add_segment(&mut w, &mut meta, 0, Axis::X);
    add_segment(&mut w, &mut meta, 1, Axis::Y);
    w.journal(&meta).unwrap();
    drop(w);
    fs::remove_file(root.path().join(".t3.tmp/c1-s1.zst")).unwrap();

    let found = recover_unsealed(root.path()).unwrap();
    assert_eq!(
        found,
        [Recovery::Failed {
            id: "t3".to_owned(),
            segments: 1
        }]
    );
    assert_eq!(
        open_session(root.path(), "t3")
            .unwrap()
            .meta()
            .segments
            .len(),
        1
    );
}

#[test]
fn a_failed_meta_write_leaves_no_meta_json_and_recovery_seals_from_the_journal() {
    let root = TempDir::new().unwrap();
    let mut meta = base("t4");
    let mut w = SessionWriter::create(root.path(), "t4", &meta).unwrap();
    add_segment(&mut w, &mut meta, 0, Axis::X);
    w.journal(&meta).unwrap();
    // A directory in the staging file's place makes the write of meta.json fail.
    let tmp = root.path().join(".t4.tmp");
    fs::create_dir(tmp.join("meta.json.tmp")).unwrap();
    assert!(w.seal(&meta).is_err());
    assert!(!tmp.join("meta.json").exists());
    drop(w);

    let found = recover_unsealed(root.path()).unwrap();
    assert!(matches!(
        found.as_slice(),
        [Recovery::Failed { segments: 1, .. }]
    ));
    assert_eq!(
        names(&root.path().join("t4")),
        ["c0-s0.zst", "c1-s0.zst", "meta.json"]
    );
}

#[test]
fn a_tmp_with_meta_json_that_never_got_its_rename_is_renamed_unchanged() {
    let root = TempDir::new().unwrap();
    let mut meta = base("t5");
    let mut w = SessionWriter::create(root.path(), "t5", &meta).unwrap();
    add_segment(&mut w, &mut meta, 0, Axis::X);
    w.journal(&meta).unwrap();
    let tmp = root.path().join(".t5.tmp");
    fs::write(tmp.join("meta.json"), meta.to_json().unwrap()).unwrap();
    drop(w);

    let found = recover_unsealed(root.path()).unwrap();
    assert_eq!(
        found,
        [Recovery::Finished {
            id: "t5".to_owned()
        }]
    );
    let session = open_session(root.path(), "t5").unwrap();
    assert_eq!(session.meta().outcome, Outcome::Complete);
    assert!(!root.path().join("t5").join(JOURNAL_FILE).exists());
}

#[test]
fn a_journal_with_no_finished_segment_is_discarded_and_reported() {
    let root = TempDir::new().unwrap();
    let w = SessionWriter::create(root.path(), "t6", &base("t6")).unwrap();
    drop(w);
    let found = recover_unsealed(root.path()).unwrap();
    assert_eq!(
        found,
        [Recovery::Discarded {
            id: "t6".to_owned()
        }]
    );
    assert!(names(root.path()).is_empty());
}

#[test]
fn a_tmp_without_a_journal_is_left_for_the_sweep_and_not_reported() {
    let root = TempDir::new().unwrap();
    let dir = root.path().join(".t7.tmp");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("c0-s0.zst"), b"x").unwrap();
    assert!(recover_unsealed(root.path()).unwrap().is_empty());
    assert_eq!(names(root.path()), [".t7.tmp"]);
    assert_eq!(sweep_leftovers(root.path()).unwrap(), [".t7.tmp"]);
    assert!(names(root.path()).is_empty());
}

#[test]
fn seal_with_a_count_that_differs_from_the_file_fails_and_keeps_the_tmp() {
    let root = TempDir::new().unwrap();
    let mut meta = base("t8");
    let mut w = SessionWriter::create(root.path(), "t8", &meta).unwrap();
    add_segment(&mut w, &mut meta, 0, Axis::X);
    meta.segments[0].files[0].sample_count += 1;
    assert!(matches!(
        w.seal(&meta),
        Err(SessionError::SampleCountMismatch { .. })
    ));
    assert!(root.path().join(".t8.tmp").exists());
    assert!(!root.path().join("t8").exists());
    w.abandon().unwrap();
    assert!(names(root.path()).is_empty());
}

#[test]
fn create_for_an_existing_session_or_tmp_is_already_exists() {
    let root = TempDir::new().unwrap();
    fs::create_dir(root.path().join("t9")).unwrap();
    assert!(matches!(
        SessionWriter::create(root.path(), "t9", &base("t9")),
        Err(SessionError::AlreadyExists { .. })
    ));
    let _w = SessionWriter::create(root.path(), "t10", &base("t10")).unwrap();
    assert!(matches!(
        SessionWriter::create(root.path(), "t10", &base("t10")),
        Err(SessionError::AlreadyExists { .. })
    ));
}

#[test]
fn a_corrupt_journal_is_skipped_and_the_other_captures_still_recover() {
    let root = TempDir::new().unwrap();
    let mut meta = base("good");
    let mut w = SessionWriter::create(root.path(), "good", &meta).unwrap();
    add_segment(&mut w, &mut meta, 0, Axis::X);
    w.journal(&meta).unwrap();
    drop(w);
    let bad = SessionWriter::create(root.path(), "bad", &base("bad")).unwrap();
    drop(bad);
    fs::write(
        root.path().join(".bad.tmp").join(JOURNAL_FILE),
        b"{ not json",
    )
    .unwrap();

    let found = recover_unsealed(root.path()).unwrap();
    assert_eq!(
        found,
        [Recovery::Failed {
            id: "good".to_owned(),
            segments: 1
        }]
    );
    assert!(root.path().join(".bad.tmp").exists());
}

/// A capture with one finished segment, abandoned as a crash would leave it.
fn crashed_capture(root: &Path, id: &str) {
    let mut meta = base(id);
    let mut w = SessionWriter::create(root, id, &meta).unwrap();
    add_segment(&mut w, &mut meta, 0, Axis::X);
    w.journal(&meta).unwrap();
}

#[test]
fn a_recovery_that_fails_keeps_the_capture_and_the_sweep_does_not_delete_it() {
    let root = TempDir::new().unwrap();
    crashed_capture(root.path(), "t11");
    // A session with the same id already stands in the way, so the rename cannot happen.
    fs::create_dir(root.path().join("t11")).unwrap();

    let found = recover_unsealed(root.path()).unwrap();
    assert!(
        matches!(found.as_slice(), [Recovery::Kept { id, .. }] if id == "t11"),
        "{found:?}"
    );
    assert!(sweep_leftovers(root.path()).unwrap().is_empty());
    let tmp = root.path().join(".t11.tmp");
    assert_eq!(names(&tmp), ["c0-s0.zst", "c1-s0.zst", JOURNAL_FILE]);

    // Once the obstacle is gone the next start recovers it.
    fs::remove_dir(root.path().join("t11")).unwrap();
    let found = recover_unsealed(root.path()).unwrap();
    assert!(matches!(
        found.as_slice(),
        [Recovery::Failed { segments: 1, .. }]
    ));
}

#[cfg(unix)]
#[test]
fn a_recovery_that_cannot_write_keeps_the_recordings_through_the_sweep() {
    use std::os::unix::fs::PermissionsExt;

    let root = TempDir::new().unwrap();
    crashed_capture(root.path(), "t12");
    let tmp = root.path().join(".t12.tmp");
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o555)).unwrap();
    if fs::write(tmp.join("probe"), b"x").is_ok() {
        // Running as a user the mode does not bind (root): the failure cannot be provoked.
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }

    let found = recover_unsealed(root.path()).unwrap();
    assert!(
        matches!(found.as_slice(), [Recovery::Kept { .. }]),
        "{found:?}"
    );
    assert!(sweep_leftovers(root.path()).unwrap().is_empty());
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(names(&tmp), ["c0-s0.zst", "c1-s0.zst", JOURNAL_FILE]);
}
