//! Session directories: write and seal, open, and load. Native only; the browser decodes the
//! same bytes through [`crate::codec`].
//!
//! A session is `<root>/<id>/` holding `meta.json` and one sample file per recording. It is
//! sealed exactly when that directory exists and holds `meta.json`.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use wobblescope_core::RawSamples;

use crate::SessionError;
use crate::codec::{decode_samples, encode_samples};
use crate::meta::{SegmentFile, SessionMeta, validate_session_id};

const META_FILE: &str = "meta.json";

/// A sealed session opened for reading.
#[derive(Debug)]
pub struct Session {
    dir: PathBuf,
    meta: SessionMeta,
}

impl Session {
    /// The validated metadata.
    pub fn meta(&self) -> &SessionMeta {
        &self.meta
    }

    /// Loads and decodes one recording. `file` must be an entry of this session's metadata, so
    /// only names the metadata validated are ever opened.
    ///
    /// # Errors
    /// [`SessionError::UnsafePath`] for an unlisted entry or a non-regular file, a decoding
    /// error, or [`SessionError::SampleCountMismatch`].
    pub fn load(&self, file: &SegmentFile) -> Result<RawSamples, SessionError> {
        let listed = self
            .meta
            .segments
            .iter()
            .flat_map(|s| &s.files)
            .any(|f| f == file);
        if !listed {
            return Err(SessionError::UnsafePath(file.sample_file.clone()));
        }
        let bytes = fs::read(regular_file(&self.dir.join(&file.sample_file))?)?;
        let samples = decode_samples(&bytes)?;
        if samples.len() != file.sample_count as usize {
            return Err(SessionError::SampleCountMismatch {
                file: file.sample_file.clone(),
                expected: file.sample_count,
                found: samples.len(),
            });
        }
        Ok(samples)
    }
}

impl Session {
    /// Total length in bytes of the regular files directly inside the session directory.
    /// Symlinks and subdirectories are ignored, and lengths are file sizes, not disk blocks.
    ///
    /// # Errors
    /// [`SessionError::Io`] when the directory cannot be read.
    pub fn size_bytes(&self) -> Result<u64, SessionError> {
        let mut total = 0;
        for entry in fs::read_dir(&self.dir)? {
            let md = fs::symlink_metadata(entry?.path())?;
            if md.is_file() {
                total += md.len();
            }
        }
        Ok(total)
    }
}

/// Returns `path` when it is a regular file and not a symlink.
fn regular_file(path: &Path) -> Result<&Path, SessionError> {
    let md = fs::symlink_metadata(path)?;
    if md.is_file() {
        Ok(path)
    } else {
        Err(SessionError::UnsafePath(path.display().to_string()))
    }
}

/// Opens the sealed session `id` under `root`.
///
/// Symlinks are refused for the session directory, `meta.json` and every sample file, so a
/// hostile link cannot make a read leave the session.
///
/// # Errors
/// [`SessionError::NotSealed`] when the directory or `meta.json` is missing,
/// [`SessionError::UnsafePath`] for a symlink, or a metadata error.
pub fn open_session(root: &Path, id: &str) -> Result<Session, SessionError> {
    validate_session_id(id)?;
    let dir = root.join(id);
    match fs::symlink_metadata(&dir) {
        Ok(md) if md.is_dir() => {}
        Ok(_) => return Err(SessionError::UnsafePath(dir.display().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(SessionError::NotSealed { id: id.to_owned() });
        }
        Err(e) => return Err(e.into()),
    }
    let meta_path = dir.join(META_FILE);
    match fs::symlink_metadata(&meta_path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(SessionError::NotSealed { id: id.to_owned() });
        }
        Err(e) => return Err(e.into()),
    }
    let text = fs::read_to_string(regular_file(&meta_path)?)?;
    let meta = SessionMeta::from_json(&text)?;
    Ok(Session { dir, meta })
}

/// Writes `meta` and the recordings `source` supplies, then seals the session by renaming its
/// temporary directory into place.
///
/// The rename is the commit point: a failure before it removes the temporary directory and
/// leaves no sealed session. `source` is called once per file entry, in metadata order, and its
/// recording must hold `sample_count` samples. The final sync of `root` is best-effort.
///
/// # Errors
/// [`SessionError::AlreadyExists`] when `<root>/<id>` exists (it is left untouched), a metadata
/// error, the source's error, or an I/O error.
pub fn write_session<S>(root: &Path, meta: &SessionMeta, mut source: S) -> Result<(), SessionError>
where
    S: FnMut(&SegmentFile) -> Result<RawSamples, SessionError>,
{
    let json = meta.to_json()?;
    let final_dir = root.join(&meta.id);
    if fs::symlink_metadata(&final_dir).is_ok() {
        return Err(SessionError::AlreadyExists {
            id: meta.id.clone(),
        });
    }
    let tmp = root.join(format!(".{}.tmp", meta.id));
    if fs::symlink_metadata(&tmp).is_ok() {
        fs::remove_dir_all(&tmp)?;
    }
    fs::create_dir(&tmp)?;
    let built = fill(&tmp, meta, &json, &mut source).and_then(|()| {
        fs::rename(&tmp, &final_dir)?;
        Ok(())
    });
    if let Err(e) = built {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    sync_dir(root);
    Ok(())
}

/// Writes every sample file, then `meta.json`, syncing each, then syncs the directory.
fn fill<S>(dir: &Path, meta: &SessionMeta, json: &str, source: &mut S) -> Result<(), SessionError>
where
    S: FnMut(&SegmentFile) -> Result<RawSamples, SessionError>,
{
    for file in meta.segments.iter().flat_map(|s| &s.files) {
        let samples = source(file)?;
        if samples.len() != file.sample_count as usize {
            return Err(SessionError::SampleCountMismatch {
                file: file.sample_file.clone(),
                expected: file.sample_count,
                found: samples.len(),
            });
        }
        write_synced(&dir.join(&file.sample_file), &encode_samples(&samples)?)?;
    }
    write_synced(&dir.join(META_FILE), json.as_bytes())?;
    sync_temp_dir(dir)?;
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), SessionError> {
    let mut f = File::options().write(true).create_new(true).open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}

/// Syncs the temporary directory before the rename, so a failed durability check stops the
/// seal. Directories cannot be opened for syncing on every platform, so only Unix checks.
fn sync_temp_dir(dir: &Path) -> Result<(), SessionError> {
    #[cfg(unix)]
    File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// Syncs `root` after the rename, best-effort: the session is already sealed by then.
fn sync_dir(dir: &Path) {
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
}

/// Names of the sealed sessions directly under `root`, sorted.
///
/// An entry counts when its name is valid UTF-8 and does not start with a dot, it is a real
/// directory (a symlink is not), and it holds `meta.json`. The metadata is not parsed, so a
/// listed session can still fail [`open_session`].
///
/// # Errors
/// [`SessionError::Io`] when `root` cannot be read.
pub fn list_sealed(root: &Path) -> Result<Vec<String>, SessionError> {
    let mut names = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') || !fs::symlink_metadata(entry.path())?.is_dir() {
            continue;
        }
        if fs::symlink_metadata(entry.path().join(META_FILE)).is_ok() {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// Deletes the sealed session `id` from `root`.
///
/// The directory is renamed to `.<id>.deleting` first, which is the commit point: from then on
/// the session no longer lists as sealed, and a crash before the files are gone leaves a
/// remnant that [`sweep_leftovers`] removes. The final sync of `root` is best-effort.
///
/// # Errors
/// [`SessionError::Invalid`] for a bad id, [`SessionError::NotSealed`] when there is no such
/// directory, [`SessionError::UnsafePath`] when it is not a real directory, or an I/O error.
pub fn remove_session(root: &Path, id: &str) -> Result<(), SessionError> {
    validate_session_id(id)?;
    let dir = root.join(id);
    match fs::symlink_metadata(&dir) {
        Ok(md) if md.is_dir() => {}
        Ok(_) => return Err(SessionError::UnsafePath(dir.display().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(SessionError::NotSealed { id: id.to_owned() });
        }
        Err(e) => return Err(e.into()),
    }
    let doomed = root.join(format!(".{id}.deleting"));
    if fs::symlink_metadata(&doomed).is_ok() {
        remove_entry(&doomed)?;
    }
    fs::rename(&dir, &doomed)?;
    sync_dir(root);
    fs::remove_dir_all(&doomed)?;
    Ok(())
}

/// Removes the remnants of crashed writes and deletes directly under `root`: entries named
/// `.<id>.tmp` or `.<id>.deleting` where `<id>` is a valid session id. A symlink with such a
/// name is unlinked, never followed. Returns the removed names, sorted.
///
/// # Errors
/// The first I/O error stops the sweep and is returned.
pub fn sweep_leftovers(root: &Path) -> Result<Vec<String>, SessionError> {
    let mut swept = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let id = name.strip_prefix('.').and_then(|n| {
            n.strip_suffix(".tmp")
                .or_else(|| n.strip_suffix(".deleting"))
        });
        if id.is_some_and(|id| validate_session_id(id).is_ok()) {
            remove_entry(&entry.path())?;
            swept.push(name);
        }
    }
    swept.sort();
    Ok(swept)
}

/// Removes a file, a symlink (without following it) or a directory tree.
fn remove_entry(path: &Path) -> Result<(), SessionError> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}
