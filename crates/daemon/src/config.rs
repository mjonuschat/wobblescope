//! The daemon's config file. For now it holds only the storage settings; later plans add their
//! own keys to the same file.

use std::path::{Path, PathBuf};

use serde::Deserialize;

const MIB: u64 = 1024 * 1024;
const DEFAULT_CAP_MIB: u64 = 2048;
const DEFAULT_HEADROOM_MIB: u64 = 256;

/// Why the config could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file could not be read.
    #[error("cannot read config {}: {source}", path.display())]
    Read {
        /// The config file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The file is not valid TOML, has an unknown key or a value of the wrong type.
    #[error("invalid config {}: {source}", path.display())]
    Parse {
        /// The config file.
        path: PathBuf,
        /// The parser's error, which names the key.
        source: toml::de::Error,
    },
    /// A value is out of range.
    #[error("invalid config {}: {key} {reason}", path.display())]
    Invalid {
        /// The config file.
        path: PathBuf,
        /// The key.
        key: &'static str,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// The config root has no parent directory to put the data directory beside.
    #[error("cannot derive a data directory from config root {:?}: it has no parent", .0)]
    NoParent(PathBuf),
}

/// The keys of the file, in the units a person writes.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    storage_dir: Option<PathBuf>,
    retention_cap_mib: Option<u64>,
    free_space_headroom_mib: Option<u64>,
}

/// The storage settings, with sizes in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Where sessions and pins live, when the config names it.
    pub storage_dir: Option<PathBuf>,
    /// Total size of retained sessions above which retention evicts.
    pub retention_cap_bytes: u64,
    /// Free space that must remain on the sessions filesystem after a test's estimate.
    pub free_space_headroom_bytes: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            storage_dir: None,
            retention_cap_bytes: DEFAULT_CAP_MIB * MIB,
            free_space_headroom_bytes: DEFAULT_HEADROOM_MIB * MIB,
        }
    }
}

/// Loads the settings from `path`, or returns the defaults when there is no path.
///
/// # Errors
/// A [`ConfigError`] naming the file, and the key for a bad value.
pub fn load(path: Option<&Path>) -> Result<Settings, ConfigError> {
    let Some(path) = path else {
        return Ok(Settings::default());
    };
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_owned(),
        source,
    })?;
    let raw: RawConfig = toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.to_owned(),
        source,
    })?;
    let invalid = |key, reason| ConfigError::Invalid {
        path: path.to_owned(),
        key,
        reason,
    };
    if raw.storage_dir.as_deref().is_some_and(|d| !d.is_absolute()) {
        return Err(invalid("storage_dir", "must be an absolute path"));
    }
    if raw.retention_cap_mib == Some(0) {
        return Err(invalid("retention_cap_mib", "must be above 0"));
    }
    let defaults = Settings::default();
    Ok(Settings {
        storage_dir: raw.storage_dir,
        retention_cap_bytes: raw
            .retention_cap_mib
            .map_or(defaults.retention_cap_bytes, |m| m.saturating_mul(MIB)),
        free_space_headroom_bytes: raw
            .free_space_headroom_mib
            .map_or(defaults.free_space_headroom_bytes, |m| {
                m.saturating_mul(MIB)
            }),
    })
}

/// The data directory Moonraker's layout implies: `wobblescope` beside the config root, which
/// Moonraker reports as e.g. `~/printer_data/config`.
///
/// # Errors
/// [`ConfigError::NoParent`] for `/` or an empty path.
pub fn default_data_dir(config_root: &Path) -> Result<PathBuf, ConfigError> {
    config_root
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.join("wobblescope"))
        .ok_or_else(|| ConfigError::NoParent(config_root.to_owned()))
}
