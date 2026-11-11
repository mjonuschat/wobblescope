//! Saved results: `<data_dir>/results/<id>.zip`, outside the retention cap.
//!
//! A save leases its session, reserves space against free space only, writes
//! `.<id>.zip.tmp` without the state lock, then renames it into place. Every read verifies the
//! manifest before returning bytes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use time::OffsetDateTime;
use wobblescope_session::meta::validate_session_id;

use super::{Inner, SaveHook, Storage, StorageError, reserved_total, result_reserved_total};
use crate::capture::iso_utc;
use crate::results::container::{ResultError, ResultFile, size_bound, write_result};
use crate::results::record::{
    RESULT_VERSION, ResultDetail, ResultRecord, ResultSummary, SaveRequest,
};

static RESULT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Space and an id held for a save in progress. Dropping it releases both.
struct ResultReservation {
    inner: Arc<Inner>,
    id: String,
}

impl Drop for ResultReservation {
    fn drop(&mut self) {
        self.inner.lock().result_reservations.remove(&self.id);
    }
}

/// Removes the temporary file unless the rename disarmed it.
struct TempFile {
    path: PathBuf,
    armed: bool,
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// The result id for a file name `<id>.zip`, or `None` for anything else.
fn id_of_file(name: &str) -> Option<&str> {
    let id = name.strip_suffix(".zip")?;
    validate_session_id(id).ok()?;
    Some(id)
}

fn temp_name(id: &str) -> String {
    format!(".{id}.zip.tmp")
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    fs::File::open(dir)?.sync_all()
}

/// Removes `.*.zip.tmp` files a crash left in `dir` and returns their relative names.
pub(super) fn sweep_temporaries(dir: &Path) -> io::Result<Vec<String>> {
    let mut swept = Vec::new();
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') && name.ends_with(".zip.tmp") {
            fs::remove_file(dir.join(&name))?;
            swept.push(format!("results/{name}"));
        }
    }
    swept.sort();
    Ok(swept)
}

/// The number and total size of the result files in `dir`.
pub(super) fn scan_usage(dir: &Path) -> io::Result<(usize, u64)> {
    let mut count = 0;
    let mut bytes = 0u64;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if id_of_file(&entry.file_name().to_string_lossy()).is_none() {
            continue;
        }
        let md = fs::symlink_metadata(entry.path())?;
        if md.is_file() {
            count += 1;
            bytes = bytes.saturating_add(md.len());
        }
    }
    Ok((count, bytes))
}

fn write_error(error: ResultError) -> StorageError {
    match error {
        ResultError::Io(e) => StorageError::Io(e),
        other => StorageError::BadRequest(other.to_string()),
    }
}

/// Maps a failure to open or verify result `id`: a missing file is not found, anything else
/// makes the result unusable.
fn read_error(id: &str, error: ResultError) -> StorageError {
    match error {
        ResultError::Io(e) if e.kind() == io::ErrorKind::NotFound => {
            StorageError::ResultNotFound(id.to_owned())
        }
        other => StorageError::ResultUnusable {
            id: id.to_owned(),
            reason: other.to_string(),
        },
    }
}

/// An id that cannot name a file in the results directory names no result.
fn validate_result_id(id: &str) -> Result<(), StorageError> {
    validate_session_id(id).map_err(|_| StorageError::ResultNotFound(id.to_owned()))
}

fn summary_of(record: &ResultRecord, size_bytes: u64) -> ResultSummary {
    ResultSummary {
        id: record.id.clone(),
        label: record.label.clone(),
        created_at: Some(record.created_at.clone()),
        session_id: Some(record.session_id.clone()),
        core_version: Some(record.core_version.clone()),
        size_bytes,
        unusable: None,
    }
}

impl Storage {
    /// Test hook: `hook` runs during every save, after the space is reserved and before the
    /// write starts, with no storage lock held.
    #[doc(hidden)]
    pub fn set_save_hook(&self, hook: impl Fn(&Storage) + Send + Sync + 'static) {
        let hook: SaveHook = Arc::new(hook);
        *crate::lock::lock(&self.inner.save_hook) = Some(hook);
    }

    /// Saves the analysis in `request` as a result zip beside the session it came from.
    ///
    /// The session is leased before anything is read and stays leased until the zip is
    /// renamed into place. Space for the zip plus headroom is reserved against free space
    /// minus every outstanding reservation; nothing is evicted and the retention cap is not
    /// consulted. The state lock is not held while writing.
    ///
    /// # Errors
    /// [`StorageError::NotFound`] for an unknown session, [`StorageError::SessionChanged`],
    /// [`StorageError::BadRequest`], [`StorageError::InsufficientDisk`], or an I/O error. No
    /// temporary file survives a failure.
    pub fn save_result(
        &self,
        session_id: &str,
        request: SaveRequest,
        now: OffsetDateTime,
    ) -> Result<ResultSummary, StorageError> {
        let inner = &self.inner;
        let _lease = self.lease(session_id)?;
        let session = self.open_sealed(session_id)?;
        let meta = session.meta();
        if meta.created_at != request.session_created_at {
            return Err(StorageError::SessionChanged(session_id.to_owned()));
        }
        if let Some(axis) = request
            .axes
            .iter()
            .find(|a| a.analysis.core_version != request.core_version)
        {
            return Err(StorageError::BadRequest(format!(
                "the {:?} analysis was computed by core {}, not {}",
                axis.axis, axis.analysis.core_version, request.core_version
            )));
        }

        let mut samples = Vec::new();
        let mut sample_sizes = Vec::new();
        for file in meta.segments.iter().flat_map(|s| &s.files) {
            sample_sizes.push(session.open_sample(file)?.metadata()?.len());
            samples.push((
                file.sample_file.clone(),
                inner.sessions_dir.join(session_id).join(&file.sample_file),
            ));
        }
        let meta_path = inner.sessions_dir.join(session_id).join("meta.json");
        if !fs::symlink_metadata(&meta_path)?.is_file() {
            return Err(StorageError::BadRequest(
                "the session's meta.json is not a regular file".to_owned(),
            ));
        }
        let meta_bytes = fs::read(&meta_path)?;

        let reservation = self.reserve_result(now, |id| {
            let record = ResultRecord {
                version: RESULT_VERSION,
                id: id.to_owned(),
                created_at: iso_utc(now),
                label: request.label.clone(),
                session_id: session_id.to_owned(),
                core_version: request.core_version.clone(),
                daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
                fit_params: request.fit_params.clone(),
                overrides: request.overrides.clone(),
                axes: request.axes.clone(),
                manifest: Default::default(),
            };
            let json_len = serde_json::to_vec(&record).map_or(0, |j| j.len());
            let bound = size_bound(json_len, meta_bytes.len() as u64, &sample_sizes);
            (record, bound)
        })?;
        let (record, id) = (reservation.1, reservation.0.id.clone());
        let _reservation = reservation.0;

        let hook = crate::lock::lock(&inner.save_hook).clone();
        if let Some(hook) = hook {
            hook(self);
        }

        let temp_path = inner.results_dir.join(temp_name(&id));
        let final_path = inner.results_dir.join(format!("{id}.zip"));
        let mut temp = TempFile {
            path: temp_path,
            armed: true,
        };
        write_result(&temp.path, &record, &meta_bytes, &samples).map_err(write_error)?;
        fs::rename(&temp.path, &final_path)?;
        temp.armed = false;
        sync_dir(&inner.results_dir)?;
        let size = fs::metadata(&final_path)?.len();
        Ok(summary_of(&record, size))
    }

    /// Picks an unused result id, builds the record for it and reserves its size bound plus
    /// headroom against free space. `build` returns the record and that bound.
    fn reserve_result(
        &self,
        now: OffsetDateTime,
        build: impl FnOnce(&str) -> (ResultRecord, u64),
    ) -> Result<(ResultReservation, ResultRecord), StorageError> {
        let inner = &self.inner;
        let stamp: String = iso_utc(now).chars().filter(char::is_ascii_digit).collect();
        let mut st = inner.lock();
        let id = loop {
            let n = RESULT_COUNTER.fetch_add(1, Ordering::Relaxed);
            let id = format!("r{stamp}-{n:06}");
            let taken = st.result_reservations.contains_key(&id)
                || fs::symlink_metadata(inner.results_dir.join(format!("{id}.zip"))).is_ok();
            if !taken {
                break id;
            }
        };
        let (record, bound) = build(&id);
        let free = (inner.probe)(&inner.results_dir)?;
        let available = free
            .saturating_sub(reserved_total(&st))
            .saturating_sub(result_reserved_total(&st));
        let need = bound.saturating_add(inner.headroom_bytes);
        if available < need {
            return Err(StorageError::InsufficientDisk { need, available });
        }
        st.result_reservations.insert(id.clone(), bound);
        drop(st);
        Ok((
            ResultReservation {
                inner: inner.clone(),
                id,
            },
            record,
        ))
    }

    /// Every saved result, newest first by `created_at`. A file that cannot be opened or
    /// verified is listed last with `unusable` set. Every file is verified against its manifest, so the cost
    /// grows with the stored bytes.
    ///
    /// # Errors
    /// [`StorageError::Io`] when the directory cannot be read.
    pub fn list_results(&self) -> Result<Vec<ResultSummary>, StorageError> {
        let mut usable = Vec::new();
        let mut unusable = Vec::new();
        for entry in fs::read_dir(&self.inner.results_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(id) = id_of_file(&name) else {
                continue;
            };
            let size_bytes = fs::symlink_metadata(entry.path())?.len();
            let opened = ResultFile::open(&entry.path()).and_then(|file| {
                file.verify()?;
                Ok(file)
            });
            match opened {
                Ok(file) => {
                    let mut summary = summary_of(file.record(), size_bytes);
                    summary.id = id.to_owned();
                    usable.push(summary);
                }
                Err(e) => unusable.push(ResultSummary {
                    id: id.to_owned(),
                    label: None,
                    created_at: None,
                    session_id: None,
                    core_version: None,
                    size_bytes,
                    unusable: Some(e.to_string()),
                }),
            }
        }
        usable.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        unusable.sort_by(|a, b| a.id.cmp(&b.id));
        usable.extend(unusable);
        Ok(usable)
    }

    /// Opens result `id` and verifies its manifest, so no bytes come from an unverified file.
    fn open_verified(&self, id: &str) -> Result<ResultFile, StorageError> {
        validate_result_id(id)?;
        let path = self.inner.results_dir.join(format!("{id}.zip"));
        let file = ResultFile::open(&path).map_err(|e| read_error(id, e))?;
        file.verify().map_err(|e| read_error(id, e))?;
        Ok(file)
    }

    /// The record and session metadata of one result, after verifying the manifest.
    ///
    /// # Errors
    /// [`StorageError::ResultNotFound`] or [`StorageError::ResultUnusable`].
    pub fn result(&self, id: &str) -> Result<ResultDetail, StorageError> {
        let file = self.open_verified(id)?;
        let meta = file.meta().map_err(|e| read_error(id, e))?;
        Ok(ResultDetail {
            record: file.record().clone(),
            meta,
        })
    }

    /// The bytes of one sample file in a result, after verifying the manifest. The file is
    /// found through the stored metadata.
    ///
    /// # Errors
    /// [`StorageError::ResultNotFound`], [`StorageError::ResultUnusable`] or
    /// [`StorageError::NoRecording`].
    pub fn result_sample(
        &self,
        id: &str,
        chip: u32,
        segment: usize,
    ) -> Result<Vec<u8>, StorageError> {
        let file = self.open_verified(id)?;
        let meta = file.meta().map_err(|e| read_error(id, e))?;
        let entry = meta
            .find_file(chip, segment)
            .ok_or_else(|| StorageError::NoRecording {
                id: id.to_owned(),
                chip,
                segment,
            })?;
        file.read_sample(&entry.sample_file)
            .map_err(|e| read_error(id, e))
    }

    /// Deletes a result, whether or not it is usable.
    ///
    /// # Errors
    /// [`StorageError::ResultNotFound`] or an I/O error.
    pub fn delete_result(&self, id: &str) -> Result<(), StorageError> {
        validate_result_id(id)?;
        match fs::remove_file(self.inner.results_dir.join(format!("{id}.zip"))) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(StorageError::ResultNotFound(id.to_owned()));
            }
            Err(e) => return Err(e.into()),
        }
        sync_dir(&self.inner.results_dir)?;
        Ok(())
    }
}
