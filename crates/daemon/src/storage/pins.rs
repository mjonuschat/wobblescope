//! `pins.json`: the ids of sessions the user pinned. Sealed sessions are immutable, so pin
//! state lives here, beside the sessions directory.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use wobblescope_session::meta::validate_session_id;

const VERSION: u32 = 1;

/// Why `pins.json` could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum PinsError {
    /// The file exists but cannot be read.
    #[error("cannot read {}: {source}", path.display())]
    Read {
        /// The pins file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The file is damaged or from a newer version. Guessing "no pins" here would let retention
    /// delete sessions the user pinned.
    #[error("{} is unusable: {reason}", path.display())]
    Unusable {
        /// The pins file.
        path: PathBuf,
        /// What is wrong with it.
        reason: String,
    },
    /// The file could not be written.
    #[error("cannot write {}: {source}", path.display())]
    Write {
        /// The pins file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
}

#[derive(Deserialize)]
struct VersionProbe {
    version: u32,
}

#[derive(Serialize, Deserialize)]
struct FileShape {
    version: u32,
    pinned: Vec<String>,
}

/// The set of pinned session ids.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Pins {
    ids: BTreeSet<String>,
}

impl Pins {
    /// Loads `path`. A missing file means no pins.
    ///
    /// # Errors
    /// [`PinsError`] for an unreadable file, bad JSON, a missing `pinned` list, an invalid id
    /// or a version other than 1.
    pub fn load(path: &Path) -> Result<Self, PinsError> {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(PinsError::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        let unusable = |reason: String| PinsError::Unusable {
            path: path.to_owned(),
            reason,
        };
        let probe: VersionProbe =
            serde_json::from_str(&text).map_err(|e| unusable(e.to_string()))?;
        if probe.version != VERSION {
            return Err(unusable(format!("unsupported version {}", probe.version)));
        }
        let shape: FileShape = serde_json::from_str(&text).map_err(|e| unusable(e.to_string()))?;
        for id in &shape.pinned {
            validate_session_id(id).map_err(|e| unusable(format!("pinned id {id:?}: {e}")))?;
        }
        Ok(Self {
            ids: shape.pinned.into_iter().collect(),
        })
    }

    /// Writes the set to `path` through a temporary file beside it, so a failed or interrupted
    /// write leaves the previous file whole.
    ///
    /// # Errors
    /// [`PinsError::Write`].
    pub fn write(&self, path: &Path) -> Result<(), PinsError> {
        let fail = |source| PinsError::Write {
            path: path.to_owned(),
            source,
        };
        let shape = FileShape {
            version: VERSION,
            pinned: self.ids.iter().cloned().collect(),
        };
        let json =
            serde_json::to_string_pretty(&shape).map_err(|e| fail(std::io::Error::other(e)))?;
        let mut tmp_name = path.as_os_str().to_owned();
        tmp_name.push(".tmp");
        let tmp = PathBuf::from(tmp_name);
        let written = (|| {
            let mut f = File::create(&tmp)?;
            f.write_all(json.as_bytes())?;
            f.sync_all()?;
            fs::rename(&tmp, path)
        })();
        if let Err(e) = written {
            let _ = fs::remove_file(&tmp);
            return Err(fail(e));
        }
        #[cfg(unix)]
        if let Some(dir) = path.parent() {
            File::open(dir).and_then(|d| d.sync_all()).map_err(fail)?;
        }
        Ok(())
    }

    /// Drops every pin for which `exists` is false and returns the dropped ids. The file is
    /// not touched until the next [`Pins::write`].
    pub fn prune(&mut self, exists: impl Fn(&str) -> bool) -> Vec<String> {
        let gone: Vec<String> = self.ids.iter().filter(|id| !exists(id)).cloned().collect();
        for id in &gone {
            self.ids.remove(id);
        }
        gone
    }

    /// Whether `id` is pinned.
    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }

    /// Pins `id`. Returns whether it was newly pinned.
    pub fn insert(&mut self, id: String) -> bool {
        self.ids.insert(id)
    }

    /// Unpins `id`. Returns whether it was pinned.
    pub fn remove(&mut self, id: &str) -> bool {
        self.ids.remove(id)
    }

    /// The pinned ids in order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.ids.iter().map(String::as_str)
    }

    /// Whether nothing is pinned.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}
