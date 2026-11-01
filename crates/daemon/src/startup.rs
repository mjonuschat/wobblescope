//! Opening storage once Moonraker names the data directory.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::config::{ConfigError, Settings, default_data_dir};
use crate::moonraker::ConfigRootReport;
use crate::moonraker::discovery::RootsError;
use crate::storage::{Probe, Storage, StorageError};

/// Holds the storage once it is open. The API reads it on every request and answers
/// `storage_unavailable` while it is empty.
pub type StorageCell = Arc<OnceLock<Storage>>;

/// An empty cell.
#[must_use]
pub fn new_cell() -> StorageCell {
    Arc::new(OnceLock::new())
}

/// Why the daemon cannot find or open its data directory. Every variant is fatal.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    /// Moonraker's answer has no usable config root.
    #[error("cannot derive the data directory: {0}")]
    Roots(#[from] RootsError),
    /// The config root is a relative path.
    #[error("cannot derive the data directory: Moonraker's config root {} is not an absolute path", .0.display())]
    NotAbsolute(std::path::PathBuf),
    /// The config root has no parent to put the data directory beside.
    #[error("cannot derive the data directory: {0}")]
    NoParent(#[from] ConfigError),
    /// Storage could not open the derived directory.
    #[error("cannot open storage at {}: {source}", dir.display())]
    Storage {
        /// The derived directory.
        dir: std::path::PathBuf,
        /// Why it failed.
        source: StorageError,
    },
    /// The supervisor stopped before Moonraker answered.
    #[error("the Moonraker connection stopped before it reported the data directory")]
    Stopped,
    /// The storage open task did not finish.
    #[error("the storage open task failed: {0}")]
    Task(String),
}

/// Opens `dir`, logging what opening found. Blocks, so callers on the runtime use
/// [`resolve_storage`] or a blocking thread.
///
/// # Errors
/// [`StartupError::Storage`] when storage cannot open.
pub fn open_storage(
    dir: &Path,
    settings: &Settings,
    probe: Probe,
) -> Result<Storage, StartupError> {
    let (storage, report) =
        Storage::open(dir, settings, probe).map_err(|source| StartupError::Storage {
            dir: dir.to_owned(),
            source,
        })?;
    eprintln!("storage: opened {}", dir.display());
    for line in report.lines() {
        eprintln!("storage: {line}");
    }
    Ok(storage)
}

/// Waits for Moonraker's config root, derives the data directory beside it, opens storage there
/// and fills `cell`. It returns once the cell is filled.
///
/// # Errors
/// A [`StartupError`] when the answer cannot give a usable directory or storage cannot open it.
pub async fn resolve_storage(
    report: ConfigRootReport,
    settings: Settings,
    probe: Probe,
    cell: StorageCell,
) -> Result<(), StartupError> {
    let root = report.await.map_err(|_| StartupError::Stopped)??;
    if !root.is_absolute() {
        return Err(StartupError::NotAbsolute(root));
    }
    let dir = default_data_dir(&root)?;
    let storage = tokio::task::spawn_blocking(move || open_storage(&dir, &settings, probe))
        .await
        .map_err(|e| StartupError::Task(e.to_string()))??;
    // Only this coordinator fills an empty cell, and only once.
    let _ = cell.set(storage);
    Ok(())
}
