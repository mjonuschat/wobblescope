#![cfg(all(feature = "fs", not(target_arch = "wasm32")))]

use std::fs;
use std::path::Path;

use tempfile::TempDir;
use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::{Coverage, Health};
use wobblescope_core::fit::Axis;
use wobblescope_session::SessionError;
use wobblescope_session::codec::encode_samples;
use wobblescope_session::fs::{
    list_sealed, open_session, remove_session, sweep_leftovers, write_session,
};
use wobblescope_session::meta::{
    ChipMeta, Outcome, ProbePoint, SegmentFile, SegmentMeta, SessionMeta, Source, TimeDomain,
    sample_file_name,
};

fn samples(n: usize, seed: f64) -> RawSamples {
    let mut s = RawSamples::default();
    for i in 0..n {
        s.t.push(41230.322878 + i as f64 * 0.000313);
        s.x.push((i as f64 * 0.37 + seed).sin() * 300.0);
        s.y.push(seed);
        s.z.push(-9806.022374);
    }
    s
}

fn meta(id: &str) -> SessionMeta {
    let file = |chip: u32, seg: usize, n: u32| SegmentFile {
        chip,
        sample_file: sample_file_name(chip as usize, seg),
        sample_count: n,
        coverage: Coverage::Complete,
    };
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
        created_at: "2026-10-31T19:00:00Z".to_owned(),
        source: Source::Capture,
        time_domain: TimeDomain::KlipperPrintTime,
        mode: None,
        settings: None,
        printer: None,
        outcome: Outcome::Complete,
        notes: vec![],
        probe_points: vec![ProbePoint { position: None }],
        chips: vec![chip("a"), chip("b")],
        segments: vec![
            SegmentMeta {
                axis: Axis::X,
                point: 0,
                window: None,
                files: vec![file(0, 0, 800), file(1, 0, 500)],
            },
            SegmentMeta {
                axis: Axis::Y,
                point: 0,
                window: None,
                files: vec![file(0, 1, 800)],
            },
        ],
    }
}

/// The samples the tests store for each file entry.
fn source(f: &SegmentFile) -> Result<RawSamples, SessionError> {
    Ok(samples(
        f.sample_count as usize,
        f.chip as f64 + f.sample_count as f64,
    ))
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    names(dir)
        .into_iter()
        .map(|n| {
            let b = fs::read(dir.join(&n)).unwrap();
            (n, b)
        })
        .collect()
}

#[test]
fn write_then_open_round_trips_meta_and_every_file() {
    let root = TempDir::new().unwrap();
    let m = meta("run1");
    write_session(root.path(), &m, source).unwrap();
    let session = open_session(root.path(), "run1").unwrap();
    assert_eq!(session.meta(), &m);
    for seg in &m.segments {
        for f in &seg.files {
            assert_eq!(session.load(f).unwrap(), source(f).unwrap());
        }
    }
    assert_eq!(
        names(&root.path().join("run1")),
        ["c0-s0.zst", "c0-s1.zst", "c1-s0.zst", "meta.json"]
    );
    assert_eq!(names(root.path()), ["run1"]);
}

#[test]
fn a_directory_without_meta_json_is_not_sealed() {
    let root = TempDir::new().unwrap();
    let dir = root.path().join("half");
    fs::create_dir(&dir).unwrap();
    fs::write(
        dir.join("c0-s0.zst"),
        encode_samples(&samples(4, 1.0)).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        open_session(root.path(), "half"),
        Err(SessionError::NotSealed { .. })
    ));
    assert!(matches!(
        open_session(root.path(), "missing"),
        Err(SessionError::NotSealed { .. })
    ));
}

#[test]
fn writing_an_existing_id_is_refused_and_leaves_it_untouched() {
    let root = TempDir::new().unwrap();
    write_session(root.path(), &meta("run1"), source).unwrap();
    let before = snapshot(&root.path().join("run1"));
    let err = write_session(root.path(), &meta("run1"), |_| Ok(samples(1, 9.0))).unwrap_err();
    assert!(matches!(err, SessionError::AlreadyExists { .. }));
    assert_eq!(snapshot(&root.path().join("run1")), before);
    assert_eq!(names(root.path()), ["run1"]);
}

#[test]
fn meta_json_is_written_after_every_sample_file_is_requested() {
    let root = TempDir::new().unwrap();
    let tmp = root.path().join(".run1.tmp");
    write_session(root.path(), &meta("run1"), |f| {
        assert!(
            !tmp.join("meta.json").exists(),
            "meta.json exists before {}",
            f.sample_file
        );
        source(f)
    })
    .unwrap();
    assert!(root.path().join("run1").join("meta.json").exists());
}

#[test]
fn a_failing_source_leaves_nothing_behind() {
    let root = TempDir::new().unwrap();
    let mut calls = 0;
    let err = write_session(root.path(), &meta("run1"), |f| {
        calls += 1;
        if calls == 2 {
            Err(SessionError::Truncated)
        } else {
            source(f)
        }
    })
    .unwrap_err();
    assert!(matches!(err, SessionError::Truncated));
    assert!(names(root.path()).is_empty());
}

#[test]
fn a_source_of_the_wrong_length_leaves_nothing_behind() {
    let root = TempDir::new().unwrap();
    let err = write_session(root.path(), &meta("run1"), |_| Ok(samples(3, 1.0))).unwrap_err();
    assert!(matches!(err, SessionError::SampleCountMismatch { .. }));
    assert!(names(root.path()).is_empty());
}

#[test]
fn a_leftover_temp_directory_does_not_block_a_later_write() {
    let root = TempDir::new().unwrap();
    let junk = root.path().join(".run1.tmp");
    fs::create_dir(&junk).unwrap();
    fs::write(junk.join("junk"), b"left by a crash").unwrap();
    write_session(root.path(), &meta("run1"), source).unwrap();
    assert_eq!(names(root.path()), ["run1"]);
    assert!(!root.path().join("run1").join("junk").exists());
}

#[test]
fn a_hand_written_meta_with_a_traversal_name_is_refused_on_open() {
    let root = TempDir::new().unwrap();
    write_session(root.path(), &meta("run1"), source).unwrap();
    let path = root.path().join("run1").join("meta.json");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("c0-s0.zst", "../escape.zst");
    fs::write(&path, text).unwrap();
    fs::write(
        root.path().join("escape.zst"),
        encode_samples(&samples(800, 1.0)).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        open_session(root.path(), "run1"),
        Err(SessionError::Invalid { .. })
    ));
}

#[test]
fn an_unsafe_id_is_refused_without_touching_the_disk() {
    let root = TempDir::new().unwrap();
    assert!(matches!(
        open_session(root.path(), "../x"),
        Err(SessionError::Invalid { .. })
    ));
    let mut m = meta("run1");
    m.id = "../x".to_owned();
    assert!(matches!(
        write_session(root.path(), &m, source),
        Err(SessionError::Invalid { .. })
    ));
    assert!(names(root.path()).is_empty());
}

#[test]
fn a_sample_file_with_fewer_samples_than_recorded_fails_to_load() {
    let root = TempDir::new().unwrap();
    write_session(root.path(), &meta("run1"), source).unwrap();
    fs::write(
        root.path().join("run1").join("c0-s0.zst"),
        encode_samples(&samples(799, 1.0)).unwrap(),
    )
    .unwrap();
    let session = open_session(root.path(), "run1").unwrap();
    let f = &session.meta().segments[0].files[0];
    assert!(matches!(
        session.load(f),
        Err(SessionError::SampleCountMismatch {
            expected: 800,
            found: 799,
            ..
        })
    ));
}

#[test]
fn a_zero_length_sample_file_fails_to_load() {
    let root = TempDir::new().unwrap();
    write_session(root.path(), &meta("run1"), source).unwrap();
    fs::write(root.path().join("run1").join("c1-s0.zst"), b"").unwrap();
    let session = open_session(root.path(), "run1").unwrap();
    let f = &session.meta().segments[0].files[1];
    assert!(session.load(f).is_err());
}

#[test]
fn a_file_entry_the_metadata_does_not_list_is_refused() {
    let root = TempDir::new().unwrap();
    write_session(root.path(), &meta("run1"), source).unwrap();
    let session = open_session(root.path(), "run1").unwrap();
    let foreign = SegmentFile {
        chip: 0,
        sample_file: "meta.json".to_owned(),
        sample_count: 1,
        coverage: Coverage::Complete,
    };
    assert!(matches!(
        session.load(&foreign),
        Err(SessionError::UnsafePath(_))
    ));
}

#[cfg(unix)]
mod symlinks {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn a_symlinked_sample_file_is_refused() {
        let root = TempDir::new().unwrap();
        write_session(root.path(), &meta("run1"), source).unwrap();
        let outside = root.path().join("outside.zst");
        fs::write(
            &outside,
            encode_samples(&source(&meta("run1").segments[0].files[0]).unwrap()).unwrap(),
        )
        .unwrap();
        let target = root.path().join("run1").join("c0-s0.zst");
        fs::remove_file(&target).unwrap();
        symlink(&outside, &target).unwrap();
        let session = open_session(root.path(), "run1").unwrap();
        let f = &session.meta().segments[0].files[0];
        assert!(matches!(session.load(f), Err(SessionError::UnsafePath(_))));
    }

    #[test]
    fn a_symlinked_meta_json_is_refused() {
        let root = TempDir::new().unwrap();
        write_session(root.path(), &meta("run1"), source).unwrap();
        let real = root.path().join("run1").join("meta.json");
        let outside = root.path().join("outside.json");
        fs::rename(&real, &outside).unwrap();
        symlink(&outside, &real).unwrap();
        assert!(matches!(
            open_session(root.path(), "run1"),
            Err(SessionError::UnsafePath(_))
        ));
    }

    #[test]
    fn a_symlinked_session_directory_is_refused() {
        let root = TempDir::new().unwrap();
        write_session(root.path(), &meta("real"), source).unwrap();
        symlink(root.path().join("real"), root.path().join("link")).unwrap();
        assert!(matches!(
            open_session(root.path(), "link"),
            Err(SessionError::UnsafePath(_))
        ));
    }
}

fn seal(root: &Path, id: &str) {
    write_session(root, &meta(id), source).unwrap();
}

#[test]
fn list_sealed_applies_the_sealed_rule() {
    let root = TempDir::new().unwrap();
    seal(root.path(), "a");
    seal(root.path(), "b");
    fs::create_dir_all(root.path().join(".a2.tmp")).unwrap();
    fs::create_dir_all(root.path().join(".b2.deleting")).unwrap();
    fs::create_dir(root.path().join("c")).unwrap();
    fs::write(root.path().join("d"), b"x").unwrap();
    fs::create_dir(root.path().join(".hidden")).unwrap();
    fs::write(root.path().join(".hidden").join("meta.json"), b"{}").unwrap();
    assert_eq!(list_sealed(root.path()).unwrap(), ["a", "b"]);
}

#[test]
fn list_sealed_of_an_empty_root_is_empty_and_of_a_missing_root_is_an_error() {
    let root = TempDir::new().unwrap();
    assert!(list_sealed(root.path()).unwrap().is_empty());
    assert!(matches!(
        list_sealed(&root.path().join("nope")),
        Err(SessionError::Io(_))
    ));
}

#[test]
fn remove_session_leaves_nothing_behind() {
    let root = TempDir::new().unwrap();
    seal(root.path(), "a");
    seal(root.path(), "b");
    remove_session(root.path(), "a").unwrap();
    assert!(matches!(
        open_session(root.path(), "a"),
        Err(SessionError::NotSealed { .. })
    ));
    assert_eq!(list_sealed(root.path()).unwrap(), ["b"]);
    assert_eq!(names(root.path()), ["b"]);
}

#[test]
fn remove_session_replaces_a_stale_deleting_directory() {
    let root = TempDir::new().unwrap();
    seal(root.path(), "a");
    let stale = root.path().join(".a.deleting");
    fs::create_dir(&stale).unwrap();
    fs::write(stale.join("junk"), b"junk").unwrap();
    remove_session(root.path(), "a").unwrap();
    assert!(names(root.path()).is_empty());
}

#[test]
fn remove_session_refuses_unknown_and_invalid_ids() {
    let root = TempDir::new().unwrap();
    let outer = TempDir::new().unwrap();
    seal(outer.path(), "x");
    assert!(matches!(
        remove_session(root.path(), "ghost"),
        Err(SessionError::NotSealed { .. })
    ));
    let nested = root.path().join("sessions");
    fs::create_dir(&nested).unwrap();
    fs::rename(outer.path().join("x"), root.path().join("x")).unwrap();
    assert!(matches!(
        remove_session(&nested, "../x"),
        Err(SessionError::Invalid { .. })
    ));
    assert!(root.path().join("x").join("meta.json").exists());
}

#[test]
fn sweep_leftovers_removes_only_crash_remnants() {
    let root = TempDir::new().unwrap();
    seal(root.path(), "a");
    let tmp = root.path().join(".a.tmp");
    fs::create_dir(&tmp).unwrap();
    fs::write(tmp.join("c0-s0.zst"), b"half").unwrap();
    let deleting = root.path().join(".b.deleting");
    fs::create_dir(&deleting).unwrap();
    fs::write(deleting.join("meta.json"), b"{}").unwrap();
    fs::write(root.path().join(".DS_Store"), b"x").unwrap();
    fs::create_dir(root.path().join(".config")).unwrap();
    fs::create_dir(root.path().join(".x y.tmp")).unwrap();
    let mut swept = sweep_leftovers(root.path()).unwrap();
    swept.sort();
    assert_eq!(swept, [".a.tmp", ".b.deleting"]);
    assert_eq!(
        names(root.path()),
        [".DS_Store", ".config", ".x y.tmp", "a"]
    );
}

#[test]
fn a_crashed_delete_is_unlisted_and_then_swept() {
    let root = TempDir::new().unwrap();
    seal(root.path(), "a");
    fs::rename(root.path().join("a"), root.path().join(".a.deleting")).unwrap();
    assert!(list_sealed(root.path()).unwrap().is_empty());
    assert_eq!(sweep_leftovers(root.path()).unwrap(), [".a.deleting"]);
    assert!(names(root.path()).is_empty());
}

#[test]
fn size_bytes_sums_meta_and_sample_files() {
    let root = TempDir::new().unwrap();
    seal(root.path(), "a");
    let session = open_session(root.path(), "a").unwrap();
    let dir = root.path().join("a");
    let expected: u64 = names(&dir)
        .iter()
        .map(|n| fs::metadata(dir.join(n)).unwrap().len())
        .sum();
    assert_eq!(session.size_bytes().unwrap(), expected);
}

#[cfg(unix)]
mod links {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn list_sealed_skips_a_symlinked_session() {
        let root = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        seal(elsewhere.path(), "e");
        seal(root.path(), "a");
        symlink(elsewhere.path().join("e"), root.path().join("e")).unwrap();
        assert_eq!(list_sealed(root.path()).unwrap(), ["a"]);
    }

    #[test]
    fn remove_session_refuses_a_symlink_and_keeps_the_target() {
        let root = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        seal(elsewhere.path(), "e");
        symlink(elsewhere.path().join("e"), root.path().join("e")).unwrap();
        assert!(matches!(
            remove_session(root.path(), "e"),
            Err(SessionError::UnsafePath(_))
        ));
        assert!(elsewhere.path().join("e").join("meta.json").exists());
    }

    #[test]
    fn sweep_leftovers_unlinks_a_symlink_without_following_it() {
        let root = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        seal(elsewhere.path(), "e");
        symlink(elsewhere.path().join("e"), root.path().join(".a.tmp")).unwrap();
        assert_eq!(sweep_leftovers(root.path()).unwrap(), [".a.tmp"]);
        assert!(names(root.path()).is_empty());
        assert!(elsewhere.path().join("e").join("meta.json").exists());
    }
}
