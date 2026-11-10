//! The saved-result zip: `result.json`, `meta.json` and `samples/<name>`, all stored.
//!
//! Entries are stored, not compressed: sample files are already one zstd frame, and a reader
//! that never inflates cannot be fed a zip bomb. `result.json` carries a SHA-256 manifest of
//! every other entry. [`ResultFile::open`] checks the structure cheaply; [`ResultFile::verify`]
//! hashes and decodes everything.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;
use wobblescope_session::codec::decode_samples;
use wobblescope_session::meta::SessionMeta;
use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use super::record::{RESULT_VERSION, ResultRecord};

/// Most entries a result may hold.
pub const MAX_ENTRIES: usize = 1024;
/// Largest `result.json`.
pub const MAX_RECORD_BYTES: u64 = 16 << 20;
/// Largest `meta.json`.
pub const MAX_META_BYTES: u64 = 4 << 20;
/// Largest sample file.
pub const MAX_SAMPLE_BYTES: u64 = 256 << 20;

const RECORD_NAME: &str = "result.json";
const META_NAME: &str = "meta.json";
const SAMPLES_PREFIX: &str = "samples/";
const COPY_CHUNK: usize = 64 * 1024;
const PER_ENTRY_OVERHEAD: u64 = 1024;
const PER_ENTRY_MANIFEST: u64 = 256;
const SLACK: u64 = 4096;

/// Why a result zip could not be written or read.
#[derive(Debug, Error)]
pub enum ResultError {
    /// The file could not be read or written.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    /// The zip breaks a structural rule.
    #[error("not a valid result: {0}")]
    Format(String),
    /// The record was written by a newer version.
    #[error("result version {0} is newer than this daemon understands")]
    TooNew(u32),
    /// An entry does not match the manifest or the metadata.
    #[error("result is damaged: {0}")]
    Corrupt(String),
    /// A JSON document in the zip is invalid.
    #[error("invalid JSON in result: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<ZipError> for ResultError {
    fn from(error: ZipError) -> Self {
        match error {
            ZipError::Io(e) => Self::Io(e),
            other => Self::Format(other.to_string()),
        }
    }
}

/// A conservative upper bound on the finished zip, for admission before writing: the payload,
/// 1 KiB per entry for headers and names, 256 bytes per entry for the manifest lines the
/// writer adds to the record, and 4 KiB.
pub fn size_bound(record_json_len: usize, meta_len: u64, sample_sizes: &[u64]) -> u64 {
    // Entries: result.json, meta.json and the samples.
    let entries = sample_sizes.len() as u64 + 2;
    record_json_len as u64
        + meta_len
        + sample_sizes.iter().sum::<u64>()
        + entries * (PER_ENTRY_OVERHEAD + PER_ENTRY_MANIFEST)
        + SLACK
}

fn entry_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::DEFAULT)
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Writes the result zip to `dest` and fsyncs it. The manifest is computed from the bytes
/// written, so `record` carries an empty one. `samples` pairs each sample file name with the
/// path to copy it from.
///
/// # Errors
/// [`ResultError::Io`] for a file that cannot be read or written, and [`ResultError::Format`]
/// for an entry over its size limit or a sample name outside the sample file pattern.
pub fn write_result(
    dest: &Path,
    record: &ResultRecord,
    meta_bytes: &[u8],
    samples: &[(String, PathBuf)],
) -> Result<(), ResultError> {
    if samples.len() + 2 > MAX_ENTRIES {
        return Err(ResultError::Format("too many entries".to_owned()));
    }
    if meta_bytes.len() as u64 > MAX_META_BYTES {
        return Err(ResultError::Format("meta.json is too large".to_owned()));
    }
    let mut manifest = BTreeMap::new();
    let mut zip = ZipWriter::new(File::create(dest)?);

    zip.start_file(META_NAME, entry_options())?;
    zip.write_all(meta_bytes)?;
    manifest.insert(META_NAME.to_owned(), sha256_hex(meta_bytes));

    for (name, source) in samples {
        let entry = format!("{SAMPLES_PREFIX}{name}");
        if !is_allowed_name(&entry) {
            return Err(ResultError::Format(format!(
                "unexpected sample name {name}"
            )));
        }
        zip.start_file(&entry, entry_options())?;
        let mut input = File::open(source)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; COPY_CHUNK];
        let mut total = 0u64;
        loop {
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > MAX_SAMPLE_BYTES {
                return Err(ResultError::Format(format!("{entry} is too large")));
            }
            hasher.update(&buf[..n]);
            zip.write_all(&buf[..n])?;
        }
        manifest.insert(entry, hex(&hasher.finalize()));
    }

    let mut finished = record.clone();
    finished.manifest = manifest;
    let json = serde_json::to_vec(&finished)?;
    if json.len() as u64 > MAX_RECORD_BYTES {
        return Err(ResultError::Format("result.json is too large".to_owned()));
    }
    zip.start_file(RECORD_NAME, entry_options())?;
    zip.write_all(&json)?;

    let file = zip.finish()?;
    file.sync_all()?;
    Ok(())
}

/// Size limits, replaceable in tests so an oversized entry need not exist on disk.
#[derive(Clone, Copy)]
struct Limits {
    record: u64,
    meta: u64,
    sample: u64,
}

const LIMITS: Limits = Limits {
    record: MAX_RECORD_BYTES,
    meta: MAX_META_BYTES,
    sample: MAX_SAMPLE_BYTES,
};

/// `result.json`, `meta.json` or `samples/c<chip>-s<segment>.zst`.
fn is_allowed_name(name: &str) -> bool {
    if name == RECORD_NAME || name == META_NAME {
        return true;
    }
    let Some(file) = name.strip_prefix(SAMPLES_PREFIX) else {
        return false;
    };
    let Some(rest) = file.strip_prefix('c').and_then(|r| r.strip_suffix(".zst")) else {
        return false;
    };
    let Some((chip, segment)) = rest.split_once("-s") else {
        return false;
    };
    let digits = |s: &str| !s.is_empty() && s.len() <= 9 && s.bytes().all(|b| b.is_ascii_digit());
    digits(chip) && digits(segment)
}

/// True when `extra` holds a zip64 extended information field (header id 1).
fn has_zip64_field(extra: &[u8]) -> bool {
    let mut rest = extra;
    while rest.len() >= 4 {
        let id = u16::from_le_bytes([rest[0], rest[1]]);
        let len = usize::from(u16::from_le_bytes([rest[2], rest[3]]));
        if id == 1 {
            return true;
        }
        rest = rest.get(4 + len..).unwrap_or(&[]);
    }
    false
}

/// True when the file ends with a zip64 end-of-central-directory locator.
fn has_zip64_locator(file: &mut File) -> io::Result<bool> {
    const EOCD: [u8; 4] = *b"PK\x05\x06";
    const LOCATOR: [u8; 4] = *b"PK\x06\x07";
    let len = file.metadata()?.len();
    let window = len.min(22 + 65535 + 20);
    file.seek(SeekFrom::Start(len - window))?;
    let mut tail = vec![0u8; window as usize];
    file.read_exact(&mut tail)?;
    file.seek(SeekFrom::Start(0))?;
    let Some(eocd) = tail.windows(4).rposition(|w| w == EOCD) else {
        return Ok(false);
    };
    Ok(eocd >= 20 && tail[eocd - 20..eocd - 16] == LOCATOR)
}

/// An opened result zip whose structure and `result.json` have been checked.
#[derive(Debug)]
pub struct ResultFile {
    path: PathBuf,
    record: ResultRecord,
    entries: BTreeSet<String>,
}

impl ResultFile {
    /// Checks the zip's structure and parses `result.json`. Nothing else is read or hashed;
    /// see [`ResultFile::verify`].
    ///
    /// # Errors
    /// [`ResultError::Format`] for a refused structure, [`ResultError::TooNew`] for a newer
    /// record and [`ResultError::Json`] for a record that does not parse.
    pub fn open(path: &Path) -> Result<Self, ResultError> {
        Self::open_with(path, LIMITS)
    }

    fn open_with(path: &Path, limits: Limits) -> Result<Self, ResultError> {
        let mut file = File::open(path)?;
        if has_zip64_locator(&mut file)? {
            return Err(ResultError::Format("zip64 is not accepted".to_owned()));
        }
        let mut archive = ZipArchive::new(file)?;
        if archive.len() > MAX_ENTRIES {
            return Err(ResultError::Format("too many entries".to_owned()));
        }
        if archive.has_duplicate_names() {
            return Err(ResultError::Format("duplicate entry names".to_owned()));
        }
        let mut entries = BTreeSet::new();
        for index in 0..archive.len() {
            let entry = archive.by_index_raw(index)?;
            let name = std::str::from_utf8(entry.name_raw())
                .map_err(|_| ResultError::Format("entry name is not UTF-8".to_owned()))?
                .to_owned();
            if name.contains("..") || name.contains('\\') || name.starts_with('/') {
                return Err(ResultError::Format(format!("unsafe entry name {name}")));
            }
            if !is_allowed_name(&name) {
                return Err(ResultError::Format(format!("unexpected entry {name}")));
            }
            if entry.is_symlink() {
                return Err(ResultError::Format(format!("{name} is a symlink")));
            }
            if entry.compression() != CompressionMethod::Stored || entry.encrypted() {
                return Err(ResultError::Format(format!("{name} is not a stored entry")));
            }
            if entry.extra_data().is_some_and(|e| has_zip64_field(&e)) {
                return Err(ResultError::Format(format!("{name} uses zip64")));
            }
            let limit = match name.as_str() {
                RECORD_NAME => limits.record,
                META_NAME => limits.meta,
                _ => limits.sample,
            };
            if entry.size() > limit || entry.compressed_size() != entry.size() {
                return Err(ResultError::Format(format!(
                    "{name} has an unacceptable size"
                )));
            }
            if !entries.insert(name.clone()) {
                return Err(ResultError::Format(format!("duplicate entry {name}")));
            }
        }
        if !entries.contains(RECORD_NAME) {
            return Err(ResultError::Format("result.json is missing".to_owned()));
        }
        let bytes = read_entry(&mut archive, RECORD_NAME, limits.record)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        if let Some(version) = value.get("version").and_then(serde_json::Value::as_u64)
            && version > u64::from(RESULT_VERSION)
        {
            return Err(ResultError::TooNew(
                u32::try_from(version).unwrap_or(u32::MAX),
            ));
        }
        let record: ResultRecord = serde_json::from_value(value)?;
        Ok(Self {
            path: path.to_owned(),
            record,
            entries,
        })
    }

    /// The parsed `result.json`.
    pub fn record(&self) -> &ResultRecord {
        &self.record
    }

    /// Hashes every entry against the manifest, requires the entry set to be the manifest keys
    /// plus `result.json`, then decodes each sample file and requires its sample count to
    /// match `meta.json`.
    ///
    /// # Errors
    /// [`ResultError::Corrupt`] naming the first entry that fails.
    pub fn verify(&self) -> Result<(), ResultError> {
        let expected: BTreeSet<&str> = self
            .record
            .manifest
            .keys()
            .map(String::as_str)
            .chain([RECORD_NAME])
            .collect();
        let actual: BTreeSet<&str> = self.entries.iter().map(String::as_str).collect();
        if let Some(name) = expected.symmetric_difference(&actual).next() {
            return Err(ResultError::Corrupt(format!(
                "entry {name} does not match the manifest"
            )));
        }
        let mut archive = self.archive()?;
        let mut contents = BTreeMap::new();
        for (name, hash) in &self.record.manifest {
            let bytes = read_entry(&mut archive, name, LIMITS.sample)?;
            if sha256_hex(&bytes) != *hash {
                return Err(ResultError::Corrupt(format!("{name} fails its hash")));
            }
            contents.insert(name.as_str(), bytes);
        }
        let meta_bytes = contents
            .get(META_NAME)
            .ok_or_else(|| ResultError::Corrupt("meta.json is not in the manifest".to_owned()))?;
        let meta = parse_meta(meta_bytes).map_err(|e| ResultError::Corrupt(e.to_string()))?;
        let counts: BTreeMap<String, u32> = meta
            .segments
            .iter()
            .flat_map(|s| &s.files)
            .map(|f| (format!("{SAMPLES_PREFIX}{}", f.sample_file), f.sample_count))
            .collect();
        for (name, bytes) in &contents {
            if !name.starts_with(SAMPLES_PREFIX) {
                continue;
            }
            let count = counts.get(*name).ok_or_else(|| {
                ResultError::Corrupt(format!("{name} is not listed in meta.json"))
            })?;
            let samples =
                decode_samples(bytes).map_err(|e| ResultError::Corrupt(format!("{name}: {e}")))?;
            if samples.t.len() != *count as usize {
                return Err(ResultError::Corrupt(format!(
                    "{name} holds {} samples, meta.json says {count}",
                    samples.t.len()
                )));
            }
        }
        if let Some(name) = counts.keys().find(|n| !contents.contains_key(n.as_str())) {
            return Err(ResultError::Corrupt(format!("{name} is missing")));
        }
        Ok(())
    }

    /// The session metadata stored in the zip.
    ///
    /// # Errors
    /// [`ResultError::Format`] when `meta.json` is absent, [`ResultError::Json`] when it does
    /// not parse.
    pub fn meta(&self) -> Result<SessionMeta, ResultError> {
        if !self.entries.contains(META_NAME) {
            return Err(ResultError::Format("meta.json is missing".to_owned()));
        }
        let bytes = read_entry(&mut self.archive()?, META_NAME, LIMITS.meta)?;
        parse_meta(&bytes)
    }

    /// The raw bytes of one sample file, such as `c0-s0.zst`. Not hashed; see
    /// [`ResultFile::verify`].
    ///
    /// # Errors
    /// [`ResultError::Format`] when the zip has no such sample.
    pub fn read_sample(&self, sample_file: &str) -> Result<Vec<u8>, ResultError> {
        let name = format!("{SAMPLES_PREFIX}{sample_file}");
        if !self.entries.contains(&name) {
            return Err(ResultError::Format(format!("{name} is missing")));
        }
        read_entry(&mut self.archive()?, &name, LIMITS.sample)
    }

    fn archive(&self) -> Result<ZipArchive<File>, ResultError> {
        Ok(ZipArchive::new(File::open(&self.path)?)?)
    }
}

fn parse_meta(bytes: &[u8]) -> Result<SessionMeta, ResultError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ResultError::Format("meta.json is not UTF-8".to_owned()))?;
    SessionMeta::from_json(text).map_err(|e| ResultError::Format(e.to_string()))
}

/// Reads one stored entry, never more than `limit` bytes.
fn read_entry(
    archive: &mut ZipArchive<File>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>, ResultError> {
    let entry = archive.by_name(name)?;
    let mut bytes = Vec::new();
    entry.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(ResultError::Format(format!("{name} is too large")));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_bound_covers_payload_headers_and_manifest_lines() {
        // 4 entries (record, meta, two samples), then the payload bytes.
        assert_eq!(
            size_bound(10, 20, &[30, 40]),
            4 * 1024 + 4 * 256 + 4096 + 100
        );
    }

    #[test]
    fn refuses_an_oversized_entry() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("big.zip");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("samples/c0-s0.zst", entry_options())
            .unwrap();
        zip.write_all(&[0u8; 100]).unwrap();
        zip.start_file(RECORD_NAME, entry_options()).unwrap();
        zip.write_all(b"{}").unwrap();
        zip.finish().unwrap();
        let limits = Limits {
            sample: 99,
            ..LIMITS
        };
        assert!(matches!(
            ResultFile::open_with(&path, limits),
            Err(ResultError::Format(_))
        ));
    }
}
