//! Session storage: where sessions live, which are pinned, and how much space they may use.
//!
//! `<data_dir>/sessions/<id>/` holds the sealed sessions and `<data_dir>/pins.json` the pins.
//! [`Storage`] is a cheap handle around one mutex-protected state. Every operation is
//! synchronous and may block on disk, including eviction deletes, so an async caller runs
//! them on a blocking thread.

pub mod admission;
pub mod capture;
pub mod estimate;
pub mod index;
pub mod pins;
pub mod probe;
mod results;

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use wobblescope_session::SessionError;
use wobblescope_session::fs::{Session, open_session, remove_session, sweep_leftovers};
use wobblescope_session::meta::SessionMeta;
use wobblescope_session::writer::{Recovery, recover_unsealed};

pub use admission::Reservation;

/// Runs `f` on a blocking thread. The error is the message for a task that panicked or was
/// cancelled.
pub(crate) async fn run_blocking<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("the storage task failed: {e}"))
}
use index::{Index, ListFilter, Skipped, Summary};
use pins::{Pins, PinsError};

use crate::config::Settings;

/// Reports the bytes available on the filesystem holding a path. Injected so tests can
/// simulate a nearly full disk.
pub type Probe = Arc<dyn Fn(&Path) -> io::Result<u64> + Send + Sync>;

/// The probe that asks the operating system.
pub fn default_probe() -> Probe {
    Arc::new(probe::available_bytes)
}

/// Why a storage operation failed or was refused.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// A session directory operation failed, or an id is invalid.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// `pins.json` is unusable or could not be written.
    #[error(transparent)]
    Pins(#[from] PinsError),
    /// A disk or free-space probe error.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    /// No indexed session has this id.
    #[error("no session {0}")]
    NotFound(String),
    /// The session is pinned.
    #[error("session {0} is pinned")]
    Pinned(String),
    /// The session has an active download.
    #[error("session {0} is being downloaded")]
    InUse(String),
    /// A session or reservation with this id exists.
    #[error("session id {0} is already in use")]
    IdTaken(String),
    /// The cap cannot fit the estimate even after evicting everything eligible.
    #[error(
        "a test needs {need} bytes but the cap is {cap}: {pinned_bytes} bytes are pinned and \
         {evictable_bytes} can be evicted"
    )]
    CapExceeded {
        /// The estimate.
        need: u64,
        /// The retention cap.
        cap: u64,
        /// Bytes held by pinned sessions.
        pinned_bytes: u64,
        /// Bytes held by sessions retention may delete.
        evictable_bytes: u64,
    },
    /// The filesystem cannot hold the estimate plus headroom.
    #[error("a test needs {need} bytes free including headroom but only {available} are available")]
    InsufficientDisk {
        /// The estimate plus headroom.
        need: u64,
        /// Free bytes after eviction, minus space promised to live reservations.
        available: u64,
    },
    /// The session has no recording for this chip and segment.
    #[error("session {id} has no recording for chip {chip} in segment {segment}")]
    NoRecording {
        /// The session.
        id: String,
        /// Chip position asked for.
        chip: u32,
        /// Segment position asked for.
        segment: usize,
    },
    /// The session a result was computed from no longer matches the one the browser analysed.
    #[error("session {0} was replaced since it was analysed")]
    SessionChanged(String),
    /// No saved result has this id.
    #[error("no result {0}")]
    ResultNotFound(String),
    /// The result file is damaged, unreadable or from a newer version.
    #[error("result {id} is unusable: {reason}")]
    ResultUnusable {
        /// The result.
        id: String,
        /// What is wrong with it.
        reason: String,
    },
    /// The request to save a result is inconsistent or too large to write.
    #[error("cannot save result: {0}")]
    BadRequest(String),
    /// The session to store does not match its reservation or cannot be indexed.
    #[error("cannot store session: {0}")]
    BadSession(String),
}

/// What opening storage found and did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct OpenReport {
    /// Session directories that could not be indexed, left on disk.
    pub skipped: Vec<Skipped>,
    /// Crash leftovers that were removed.
    pub swept: Vec<String>,
    /// Captures a crash left unfinished, and what recovery did with each.
    pub recovered: Vec<Recovery>,
    /// Pins dropped because their session no longer exists.
    pub pruned_pins: Vec<String>,
}

impl OpenReport {
    /// Whether startup found a capture that a crash cut short.
    #[must_use]
    pub fn unfinished_capture_found(&self) -> bool {
        !self.recovered.is_empty()
    }

    /// One human-readable line per item.
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for s in &self.skipped {
            out.push(format!(
                "skipped session directory {}: {}",
                s.name, s.reason
            ));
        }
        for r in &self.recovered {
            out.push(match r {
                Recovery::Finished { id } => {
                    format!("finished sealing capture {id}, which a crash interrupted")
                }
                Recovery::Failed { id, segments } => format!(
                    "recovered unfinished capture {id} as a failed session with {segments} \
                     segment(s)"
                ),
                Recovery::Discarded { id } => {
                    format!("discarded unfinished capture {id}: no segment had finished")
                }
                Recovery::Kept { id, reason } => format!(
                    "could not recover unfinished capture {id}: {reason}; its files are kept for \
                     the next start"
                ),
            });
        }
        for n in &self.swept {
            out.push(format!("removed leftover {n}"));
        }
        for p in &self.pruned_pins {
            out.push(format!("dropped pin for missing session {p}"));
        }
        out
    }
}

/// Storage use, for "storage use against the cap".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Usage {
    /// Bytes used by indexed sessions.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub used_bytes: u64,
    /// Bytes promised to admitted tests that have not been stored yet.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub reserved_bytes: u64,
    /// Bytes used by pinned sessions.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub pinned_bytes: u64,
    /// The retention cap.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub cap_bytes: u64,
    /// Free bytes on the sessions filesystem.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub free_bytes: u64,
    /// Number of indexed sessions.
    pub session_count: usize,
    /// Number of saved result files. Results are outside the retention cap.
    pub result_count: usize,
    /// Bytes used by saved result files.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub result_bytes: u64,
}

struct State {
    index: Index,
    pins: Pins,
    leases: HashMap<String, usize>,
    reservations: HashMap<String, u64>,
    /// Space promised to saves in progress. Counts toward free space, never toward the cap.
    result_reservations: HashMap<String, u64>,
}

struct Inner {
    sessions_dir: PathBuf,
    results_dir: PathBuf,
    save_hook: Mutex<Option<SaveHook>>,
    pins_path: PathBuf,
    cap_bytes: u64,
    headroom_bytes: u64,
    probe: Probe,
    state: Mutex<State>,
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, State> {
        crate::lock::lock(&self.state)
    }
}

/// Called by a save after its space is reserved and before it writes, so a test can act while
/// the save holds its lease.
type SaveHook = Arc<dyn Fn(&Storage) + Send + Sync>;

/// The storage layer. Cloning shares one state.
#[derive(Clone)]
pub struct Storage {
    inner: Arc<Inner>,
}

impl Storage {
    /// Opens storage under `data_dir`: creates the directories, loads the pins, removes crash
    /// leftovers and indexes the sealed sessions. Retention does not run here, so a lowered
    /// cap deletes nothing until the next admission.
    ///
    /// # Errors
    /// [`StorageError`], including an unusable `pins.json`, which stops the open before
    /// anything is deleted.
    pub fn open(
        data_dir: &Path,
        settings: &Settings,
        probe: Probe,
    ) -> Result<(Self, OpenReport), StorageError> {
        let sessions_dir = data_dir.join("sessions");
        let pins_path = data_dir.join("pins.json");
        let results_dir = data_dir.join("results");
        fs::create_dir_all(&sessions_dir)?;
        fs::create_dir_all(&results_dir)?;
        let mut pins = Pins::load(&pins_path)?;
        let recovered = recover_unsealed(&sessions_dir)?;
        let mut swept = sweep_leftovers(&sessions_dir)?;
        swept.extend(results::sweep_temporaries(&results_dir)?);
        let pruned_pins = pins.prune(|id| fs::symlink_metadata(sessions_dir.join(id)).is_ok());
        let (index, skipped) = index::scan(&sessions_dir, &pins)?;
        let storage = Self {
            inner: Arc::new(Inner {
                sessions_dir,
                results_dir,
                save_hook: Mutex::new(None),
                pins_path,
                cap_bytes: settings.retention_cap_bytes,
                headroom_bytes: settings.free_space_headroom_bytes,
                probe,
                state: Mutex::new(State {
                    index,
                    pins,
                    leases: HashMap::new(),
                    reservations: HashMap::new(),
                    result_reservations: HashMap::new(),
                }),
            }),
        };
        Ok((
            storage,
            OpenReport {
                skipped,
                swept,
                recovered,
                pruned_pins,
            },
        ))
    }

    /// Sessions passing `filter`, newest first.
    pub fn list(&self, filter: &ListFilter) -> Vec<Summary> {
        self.inner.lock().index.list(filter)
    }

    /// The summary of one session.
    pub fn get(&self, id: &str) -> Option<Summary> {
        self.inner.lock().index.get(id).cloned()
    }

    /// The summary and the stored metadata of one session.
    ///
    /// # Errors
    /// [`StorageError::NotFound`] for an unknown id, a skipped directory, or a session removed
    /// from disk since the index was built; otherwise the error from reading it.
    pub fn session(&self, id: &str) -> Result<(Summary, SessionMeta), StorageError> {
        let summary = self
            .get(id)
            .ok_or_else(|| StorageError::NotFound(id.to_owned()))?;
        let session = self.open_sealed(id)?;
        Ok((summary, session.meta().clone()))
    }

    /// Opens a session's directory, treating one that vanished since the scan as not found.
    fn open_sealed(&self, id: &str) -> Result<Session, StorageError> {
        match open_session(&self.inner.sessions_dir, id) {
            Ok(session) => Ok(session),
            Err(SessionError::NotSealed { .. }) => Err(StorageError::NotFound(id.to_owned())),
            Err(e) => Err(e.into()),
        }
    }

    /// Opens the sample file of one recording and holds a download lease on its session until
    /// the returned guard drops, so delete and eviction skip the session meanwhile. The lease
    /// is taken before the session is opened.
    ///
    /// # Errors
    /// [`StorageError::NotFound`], [`StorageError::NoRecording`] for a pair the session did not
    /// record, or the error from opening the file. No lease outlives a failure.
    pub fn open_sample(
        &self,
        id: &str,
        chip: u32,
        segment: usize,
    ) -> Result<SampleGuard, StorageError> {
        let lease = self.lease(id)?;
        let session = self.open_sealed(id)?;
        let entry =
            session
                .meta()
                .find_file(chip, segment)
                .ok_or_else(|| StorageError::NoRecording {
                    id: id.to_owned(),
                    chip,
                    segment,
                })?;
        let file = session.open_sample(entry)?;
        Ok(SampleGuard {
            file,
            _lease: lease,
        })
    }

    /// Storage use against the cap.
    ///
    /// # Errors
    /// [`StorageError::Io`] when the free-space probe fails.
    pub fn usage(&self) -> Result<Usage, StorageError> {
        let (result_count, result_bytes) = results::scan_usage(&self.inner.results_dir)?;
        let st = self.inner.lock();
        let free_bytes = (self.inner.probe)(&self.inner.sessions_dir)?;
        Ok(Usage {
            used_bytes: st.index.used_bytes(),
            reserved_bytes: reserved_total(&st),
            pinned_bytes: st.index.pinned_bytes(),
            cap_bytes: self.inner.cap_bytes,
            free_bytes,
            session_count: st.index.len(),
            result_count,
            result_bytes,
        })
    }

    /// Pins a session. The file is written before memory changes, so a failed write leaves
    /// the session unpinned. Pinning a pinned session succeeds without writing.
    ///
    /// # Errors
    /// [`StorageError::NotFound`] or a write error.
    pub fn pin(&self, id: &str) -> Result<(), StorageError> {
        let mut st = self.inner.lock();
        if st.index.get(id).is_none() {
            return Err(StorageError::NotFound(id.to_owned()));
        }
        if st.pins.contains(id) {
            return Ok(());
        }
        let mut next = st.pins.clone();
        next.insert(id.to_owned());
        next.write(&self.inner.pins_path)?;
        st.pins = next;
        st.index.set_pinned(id, true);
        Ok(())
    }

    /// Unpins a session. Unpinning an unpinned session succeeds without writing.
    ///
    /// # Errors
    /// [`StorageError::NotFound`] or a write error.
    pub fn unpin(&self, id: &str) -> Result<(), StorageError> {
        let mut st = self.inner.lock();
        if !st.pins.contains(id) {
            return if st.index.get(id).is_some() {
                Ok(())
            } else {
                Err(StorageError::NotFound(id.to_owned()))
            };
        }
        let mut next = st.pins.clone();
        next.remove(id);
        next.write(&self.inner.pins_path)?;
        st.pins = next;
        st.index.set_pinned(id, false);
        Ok(())
    }

    /// Deletes a session.
    ///
    /// # Errors
    /// [`StorageError::NotFound`], [`StorageError::Pinned`], [`StorageError::InUse`] or the
    /// error from removing it.
    pub fn delete(&self, id: &str) -> Result<(), StorageError> {
        let mut st = self.inner.lock();
        let Some(summary) = st.index.get(id) else {
            return Err(StorageError::NotFound(id.to_owned()));
        };
        if summary.pinned {
            return Err(StorageError::Pinned(id.to_owned()));
        }
        if leased(&st, id) {
            return Err(StorageError::InUse(id.to_owned()));
        }
        remove_and_forget(&self.inner, &mut st, id)
    }

    /// Marks a session as being downloaded until the returned guard drops. Delete and
    /// eviction skip it. Leases do not survive a restart.
    ///
    /// # Errors
    /// [`StorageError::NotFound`].
    pub fn lease(&self, id: &str) -> Result<DownloadLease, StorageError> {
        let mut st = self.inner.lock();
        if st.index.get(id).is_none() {
            return Err(StorageError::NotFound(id.to_owned()));
        }
        *st.leases.entry(id.to_owned()).or_insert(0) += 1;
        Ok(DownloadLease {
            inner: self.inner.clone(),
            id: id.to_owned(),
        })
    }
}

/// An open sample file that keeps its session leased while it exists. Read from it, then drop
/// it.
pub struct SampleGuard {
    file: fs::File,
    _lease: DownloadLease,
}

impl io::Read for SampleGuard {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.file.read_to_end(buf)
    }
}

/// A claim that a session is being downloaded. Dropping it releases the claim.
pub struct DownloadLease {
    inner: Arc<Inner>,
    id: String,
}

impl Drop for DownloadLease {
    fn drop(&mut self) {
        let mut st = self.inner.lock();
        if let Some(n) = st.leases.get_mut(&self.id) {
            *n -= 1;
            if *n == 0 {
                st.leases.remove(&self.id);
            }
        }
    }
}

fn leased(st: &State, id: &str) -> bool {
    st.leases.get(id).is_some_and(|n| *n > 0)
}

fn reserved_total(st: &State) -> u64 {
    st.reservations
        .values()
        .fold(0, |sum, b| sum.saturating_add(*b))
}

fn result_reserved_total(st: &State) -> u64 {
    st.result_reservations
        .values()
        .fold(0, |sum, b| sum.saturating_add(*b))
}

/// Removes a session through the crash-safe path and drops its summary once its directory is
/// gone. A rename that succeeded is a completed delete even if removing the files failed: the
/// next start sweeps the remnant. The error is still returned.
fn remove_and_forget(inner: &Inner, st: &mut State, id: &str) -> Result<(), StorageError> {
    let result = remove_session(&inner.sessions_dir, id);
    if fs::symlink_metadata(inner.sessions_dir.join(id)).is_err() {
        st.index.remove(id);
    }
    result.map_err(StorageError::from)
}
