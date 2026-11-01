//! Writing a session one segment at a time, and recovering one that was cut short.
//!
//! A capture lives in `<root>/.<id>.tmp/` while it runs. Each finished segment's files are
//! written and synced, then `meta.partial.json` (the journal) is rewritten to list them. Sealing
//! writes `meta.json` and renames the directory into place, which is the commit point. After a
//! crash, [`recover_unsealed`] turns a leftover tmp directory with a journal into a sealed
//! session whose outcome is failed.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::Coverage;

use crate::SessionError;
use crate::fs::{META_FILE, regular_file, remove_entry, sync_dir, sync_temp_dir, write_sample};
use crate::meta::{Outcome, SegmentFile, SessionMeta, sample_file_name, validate_session_id};

/// The journal: the metadata as of the last finished segment.
pub const JOURNAL_FILE: &str = "meta.partial.json";

/// The reason recovery records for a capture the daemon did not finish.
pub const INTERRUPTED_REASON: &str = "the daemon stopped while the test was running";

/// A session being written. Dropping it leaves the tmp directory for [`recover_unsealed`] or
/// [`SessionWriter::abandon`].
#[derive(Debug)]
pub struct SessionWriter {
    root: PathBuf,
    id: String,
    tmp: PathBuf,
    final_dir: PathBuf,
    written: HashMap<String, u32>,
}

impl SessionWriter {
    /// Creates `.<id>.tmp` under `root` and writes the first journal from `meta`, which should
    /// list no segment yet, so every capture directory that exists has a journal.
    ///
    /// # Errors
    /// [`SessionError::AlreadyExists`] when the session or its tmp directory exists, a metadata
    /// error, or an I/O error. A failure after the directory was made removes it again.
    pub fn create(root: &Path, id: &str, meta: &SessionMeta) -> Result<Self, SessionError> {
        validate_session_id(id)?;
        let final_dir = root.join(id);
        let tmp = root.join(format!(".{id}.tmp"));
        if fs::symlink_metadata(&final_dir).is_ok() || fs::symlink_metadata(&tmp).is_ok() {
            return Err(SessionError::AlreadyExists { id: id.to_owned() });
        }
        let writer = Self {
            root: root.to_owned(),
            id: id.to_owned(),
            tmp,
            final_dir,
            written: HashMap::new(),
        };
        let json = meta.to_json()?;
        fs::create_dir(&writer.tmp)?;
        if let Err(e) = publish(&writer.tmp, JOURNAL_FILE, &json) {
            let _ = fs::remove_dir_all(&writer.tmp);
            return Err(e);
        }
        Ok(writer)
    }

    /// The session id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Writes the files of segment `index`, one per `(chip, samples, coverage)`, synced, and
    /// returns their metadata entries. The journal is not updated; call
    /// [`SessionWriter::journal`] once the segment's metadata is complete.
    ///
    /// # Errors
    /// An encoding or I/O error. Files already written stay for the caller to abandon.
    pub fn write_segment(
        &mut self,
        index: usize,
        recordings: &[(u32, &RawSamples, Coverage)],
    ) -> Result<Vec<SegmentFile>, SessionError> {
        let mut files = Vec::with_capacity(recordings.len());
        for (chip, samples, coverage) in recordings {
            let name = sample_file_name(*chip as usize, index);
            write_sample(&self.tmp, &name, samples)?;
            let count = u32::try_from(samples.len()).map_err(|_| SessionError::TooManySamples {
                claimed: samples.len() as u64,
            })?;
            self.written.insert(name.clone(), count);
            files.push(SegmentFile {
                chip: *chip,
                sample_file: name,
                sample_count: count,
                coverage: coverage.clone(),
            });
        }
        sync_temp_dir(&self.tmp)?;
        Ok(files)
    }

    /// Rewrites the journal to `meta`, atomically.
    ///
    /// # Errors
    /// A metadata or I/O error. The previous journal is intact after a failure.
    pub fn journal(&self, meta: &SessionMeta) -> Result<(), SessionError> {
        self.check_id(meta)?;
        publish(&self.tmp, JOURNAL_FILE, &meta.to_json()?)
    }

    /// Seals the session: checks that every file `meta` names is there with the recorded count,
    /// writes `meta.json` atomically and renames the directory into place.
    ///
    /// # Errors
    /// A metadata error, [`SessionError::SampleCountMismatch`] or [`SessionError::UnsafePath`]
    /// for a file that is missing or differs, or an I/O error. The tmp directory remains after
    /// a failure, for [`SessionWriter::abandon`] or recovery.
    pub fn seal(&self, meta: &SessionMeta) -> Result<(), SessionError> {
        self.check_id(meta)?;
        let json = meta.to_json()?;
        for file in meta.segments.iter().flat_map(|s| &s.files) {
            regular_file(&self.tmp.join(&file.sample_file))?;
            if let Some(count) = self.written.get(&file.sample_file)
                && *count != file.sample_count
            {
                return Err(SessionError::SampleCountMismatch {
                    file: file.sample_file.clone(),
                    expected: file.sample_count,
                    found: *count as usize,
                });
            }
        }
        if fs::symlink_metadata(&self.final_dir).is_ok() {
            return Err(SessionError::AlreadyExists {
                id: self.id.clone(),
            });
        }
        publish(&self.tmp, META_FILE, &json)?;
        sync_temp_dir(&self.tmp)?;
        fs::rename(&self.tmp, &self.final_dir)?;
        let _ = fs::remove_file(self.final_dir.join(JOURNAL_FILE));
        sync_dir(&self.root);
        Ok(())
    }

    /// Removes the tmp directory.
    ///
    /// # Errors
    /// An I/O error.
    pub fn abandon(self) -> Result<(), SessionError> {
        if fs::symlink_metadata(&self.tmp).is_ok() {
            remove_entry(&self.tmp)?;
        }
        Ok(())
    }

    fn check_id(&self, meta: &SessionMeta) -> Result<(), SessionError> {
        if meta.id == self.id {
            Ok(())
        } else {
            Err(SessionError::Invalid {
                field: "id".to_owned(),
                reason: format!(
                    "the writer is for {:?}, the metadata names {:?}",
                    self.id, meta.id
                ),
            })
        }
    }
}

/// Writes `<dir>/<name>` atomically: to `<name>.tmp` in the same directory, synced, then
/// renamed. A full disk can leave the `.tmp` file but never a truncated file under `name`.
fn publish(dir: &Path, name: &str, text: &str) -> Result<(), SessionError> {
    let staging = dir.join(format!("{name}.tmp"));
    let mut f = File::options()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&staging)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    fs::rename(&staging, dir.join(name))?;
    Ok(())
}

/// What recovery did with one leftover capture directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recovery {
    /// A seal that crashed before its rename was finished; the outcome is as written.
    Finished {
        /// The session id.
        id: String,
    },
    /// An unfinished capture was sealed as failed with its finished segments.
    Failed {
        /// The session id.
        id: String,
        /// How many segments it kept.
        segments: usize,
    },
    /// An unfinished capture had no finished segment and was removed.
    Discarded {
        /// The session id.
        id: String,
    },
    /// A capture with finished segments could not be recovered, for example because the disk
    /// is full. Its directory is kept as it is, and the next start tries again.
    Kept {
        /// The session id.
        id: String,
        /// Why recovery failed.
        reason: String,
    },
}

/// Finishes or fails the capture directories a crash left under `root`.
///
/// A `.<id>.tmp` that holds a valid `meta.json` is renamed into place unchanged. One that holds
/// a valid journal keeps the longest run of segments whose files all exist, is sealed with
/// outcome [`Outcome::Failed`] when any remain, and is removed otherwise. A directory with no
/// readable journal is left for [`crate::fs::sweep_leftovers`] and not reported. Stray
/// `*.tmp` files and unlisted files inside a kept directory are removed. One bad directory
/// never stops the others.
///
/// # Errors
/// [`SessionError::Io`] when `root` cannot be read.
pub fn recover_unsealed(root: &Path) -> Result<Vec<Recovery>, SessionError> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Some(id) = name
            .strip_prefix('.')
            .and_then(|n| n.strip_suffix(".tmp"))
            .filter(|id| validate_session_id(id).is_ok())
        else {
            continue;
        };
        if !fs::symlink_metadata(entry.path()).is_ok_and(|m| m.is_dir()) {
            continue;
        }
        if let Some(done) = recover_one(root, id, &entry.path()) {
            found.push(done);
        }
    }
    found.sort_by(|a, b| id_of(a).cmp(id_of(b)));
    Ok(found)
}

fn id_of(r: &Recovery) -> &str {
    match r {
        Recovery::Finished { id }
        | Recovery::Failed { id, .. }
        | Recovery::Discarded { id }
        | Recovery::Kept { id, .. } => id,
    }
}

fn read_meta(dir: &Path, file: &str) -> Option<SessionMeta> {
    let text = fs::read_to_string(regular_file(&dir.join(file)).ok()?).ok()?;
    let meta = SessionMeta::from_json(&text).ok()?;
    let ids_match = dir
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == format!(".{}.tmp", meta.id));
    ids_match.then_some(meta)
}

fn files_exist(dir: &Path, files: &[SegmentFile]) -> bool {
    files
        .iter()
        .all(|f| regular_file(&dir.join(&f.sample_file)).is_ok())
}

/// Whether `dir` is a capture directory that holds recorded data a recovery could keep: a valid
/// `meta.json`, or a valid journal that lists at least one segment whose files exist. Such a
/// directory must never be swept away, even when recovering it fails.
pub(crate) fn holds_recoverable_capture(dir: &Path) -> bool {
    if read_meta(dir, META_FILE)
        .is_some_and(|m| m.segments.iter().all(|s| files_exist(dir, &s.files)))
    {
        return true;
    }
    read_meta(dir, JOURNAL_FILE).is_some_and(|m| {
        m.segments
            .first()
            .is_some_and(|s| files_exist(dir, &s.files))
    })
}

fn recover_one(root: &Path, id: &str, dir: &Path) -> Option<Recovery> {
    let kept = |reason: String| {
        Some(Recovery::Kept {
            id: id.to_owned(),
            reason,
        })
    };
    let final_dir = root.join(id);
    if fs::symlink_metadata(&final_dir).is_ok() {
        return holds_recoverable_capture(dir).then(|| Recovery::Kept {
            id: id.to_owned(),
            reason: "a session with this id already exists".to_owned(),
        });
    }
    if let Some(meta) = read_meta(dir, META_FILE)
        && meta.segments.iter().all(|s| files_exist(dir, &s.files))
    {
        clean(dir, &meta);
        if let Err(e) = fs::rename(dir, &final_dir) {
            return kept(e.to_string());
        }
        let _ = fs::remove_file(final_dir.join(JOURNAL_FILE));
        sync_dir(root);
        return Some(Recovery::Finished { id: id.to_owned() });
    }
    let mut meta = read_meta(dir, JOURNAL_FILE)?;
    // A segment's files are named by its position, so only an unbroken prefix can be kept.
    let keep = meta
        .segments
        .iter()
        .take_while(|s| files_exist(dir, &s.files))
        .count();
    if keep == 0 {
        fs::remove_dir_all(dir).ok()?;
        return Some(Recovery::Discarded { id: id.to_owned() });
    }
    meta.segments.truncate(keep);
    meta.outcome = Outcome::Failed {
        reason: INTERRUPTED_REASON.to_owned(),
    };
    let json = match meta.to_json() {
        Ok(json) => json,
        Err(e) => return kept(e.to_string()),
    };
    clean(dir, &meta);
    let sealed = publish(dir, META_FILE, &json)
        .and_then(|()| sync_temp_dir(dir))
        .and_then(|()| fs::rename(dir, &final_dir).map_err(SessionError::from));
    if let Err(e) = sealed {
        return kept(e.to_string());
    }
    let _ = fs::remove_file(final_dir.join(JOURNAL_FILE));
    sync_dir(root);
    Some(Recovery::Failed {
        id: id.to_owned(),
        segments: keep,
    })
}

/// Removes what `meta` does not list: stray `.tmp` files and files of unfinished segments.
fn clean(dir: &Path, meta: &SessionMeta) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let listed = name == META_FILE
            || name == JOURNAL_FILE
            || meta
                .segments
                .iter()
                .flat_map(|s| &s.files)
                .any(|f| f.sample_file == name);
        if !listed {
            let _ = remove_entry(&entry.path());
        }
    }
}
