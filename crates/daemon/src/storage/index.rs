//! The in-memory session index: one summary per sealed session, rebuilt at startup from each
//! session's `meta.json`. It is a cache, not a store: nothing here is written to disk.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use wobblescope_session::SessionError;
use wobblescope_session::fs::{list_sealed, open_session};
use wobblescope_session::meta::{Mode, Outcome, SessionMeta, Source};

use super::pins::Pins;

/// Where a session came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SourceKind {
    /// Recorded by a test.
    Capture,
    /// Converted from a Klipper raw CSV.
    CsvImport,
}

/// How a session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OutcomeKind {
    /// Every segment was recorded.
    Complete,
    /// Sealed with part of the data missing.
    Incomplete,
    /// The test failed; the data it had is kept.
    Failed,
}

/// What listing needs to know about one session.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    /// Session id, also its directory name.
    pub id: String,
    /// The writer's RFC 3339 timestamp, as recorded.
    pub created_at: String,
    /// `created_at` as seconds since the Unix epoch, used for ordering and range filters.
    pub created_at_unix: i64,
    /// Capture or import.
    pub source: SourceKind,
    /// How the session ended.
    pub outcome: OutcomeKind,
    /// Test mode, when recorded.
    pub mode: Option<Mode>,
    /// Printer name, when recorded.
    pub printer: Option<String>,
    /// Names of the chips in the session.
    pub chips: Vec<String>,
    /// Bytes the session occupies on disk.
    pub size_bytes: u64,
    /// Whether the user pinned it.
    pub pinned: bool,
}

/// Parses an RFC 3339 timestamp into Unix seconds.
///
/// # Errors
/// A message naming `created_at` when the text is not RFC 3339.
pub fn created_at_unix(text: &str) -> Result<i64, String> {
    OffsetDateTime::parse(text, &Rfc3339)
        .map(OffsetDateTime::unix_timestamp)
        .map_err(|e| format!("created_at {text:?} is not RFC 3339: {e}"))
}

impl Summary {
    /// Summarizes `meta`, which occupies `size_bytes` on disk.
    ///
    /// # Errors
    /// A message naming `created_at` when it does not parse.
    pub fn from_meta(meta: &SessionMeta, size_bytes: u64, pinned: bool) -> Result<Self, String> {
        Ok(Self {
            id: meta.id.clone(),
            created_at: meta.created_at.clone(),
            created_at_unix: created_at_unix(&meta.created_at)?,
            source: match meta.source {
                Source::Capture => SourceKind::Capture,
                Source::CsvImport { .. } => SourceKind::CsvImport,
            },
            outcome: match meta.outcome {
                Outcome::Complete => OutcomeKind::Complete,
                Outcome::Incomplete { .. } => OutcomeKind::Incomplete,
                Outcome::Failed { .. } => OutcomeKind::Failed,
            },
            mode: meta.mode,
            printer: meta.printer.as_ref().and_then(|p| p.name.clone()),
            chips: meta.chips.iter().map(|c| c.name.clone()).collect(),
            size_bytes,
            pinned,
        })
    }
}

/// Listing filters, combined with AND.
#[derive(Debug, Default, Clone, Serialize)]
pub struct ListFilter {
    /// Keep sessions created at or after this Unix time.
    pub created_from: Option<i64>,
    /// Keep sessions created before this Unix time.
    pub created_to: Option<i64>,
    /// Keep sessions with a chip of this name.
    pub chip: Option<String>,
    /// Keep sessions recorded on a printer of this name.
    pub printer: Option<String>,
    /// Keep pinned (true) or unpinned (false) sessions.
    pub pinned: Option<bool>,
}

impl ListFilter {
    fn keeps(&self, s: &Summary) -> bool {
        self.created_from.is_none_or(|t| s.created_at_unix >= t)
            && self.created_to.is_none_or(|t| s.created_at_unix < t)
            && self.chip.as_ref().is_none_or(|c| s.chips.contains(c))
            && self
                .printer
                .as_ref()
                .is_none_or(|p| s.printer.as_ref() == Some(p))
            && self.pinned.is_none_or(|p| s.pinned == p)
    }
}

/// A session directory the scan could not index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// The directory name.
    pub name: String,
    /// Why it was skipped.
    pub reason: String,
}

/// The summaries of every indexed session, by id.
#[derive(Debug, Default)]
pub struct Index {
    sessions: BTreeMap<String, Summary>,
}

impl Index {
    /// Sessions passing `filter`, newest first, ties by id.
    pub fn list(&self, filter: &ListFilter) -> Vec<Summary> {
        let mut out: Vec<Summary> = self
            .sessions
            .values()
            .filter(|s| filter.keeps(s))
            .cloned()
            .collect();
        out.sort_by(|a, b| {
            b.created_at_unix
                .cmp(&a.created_at_unix)
                .then_with(|| a.id.cmp(&b.id))
        });
        out
    }

    /// The summary of `id`.
    pub fn get(&self, id: &str) -> Option<&Summary> {
        self.sessions.get(id)
    }

    /// Adds or replaces a summary.
    pub fn insert(&mut self, summary: Summary) {
        self.sessions.insert(summary.id.clone(), summary);
    }

    /// Drops the summary of `id`.
    pub fn remove(&mut self, id: &str) -> Option<Summary> {
        self.sessions.remove(id)
    }

    /// Records whether `id` is pinned.
    pub fn set_pinned(&mut self, id: &str, pinned: bool) {
        if let Some(s) = self.sessions.get_mut(id) {
            s.pinned = pinned;
        }
    }

    /// All summaries.
    pub fn iter(&self) -> impl Iterator<Item = &Summary> {
        self.sessions.values()
    }

    /// Number of indexed sessions.
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Whether nothing is indexed.
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Bytes used by every indexed session.
    pub fn used_bytes(&self) -> u64 {
        self.sessions.values().map(|s| s.size_bytes).sum()
    }

    /// Bytes used by pinned sessions.
    pub fn pinned_bytes(&self) -> u64 {
        self.sessions
            .values()
            .filter(|s| s.pinned)
            .map(|s| s.size_bytes)
            .sum()
    }
}

/// Indexes every sealed session under `root`. Only `meta.json` and file sizes are read, never
/// samples. A directory that cannot be opened, whose metadata names another id, or whose
/// `created_at` does not parse is reported and left untouched.
///
/// # Errors
/// [`SessionError::Io`] when `root` itself cannot be listed.
pub fn scan(root: &Path, pins: &Pins) -> Result<(Index, Vec<Skipped>), SessionError> {
    let mut index = Index::default();
    let mut skipped = Vec::new();
    for name in list_sealed(root)? {
        match summarize_dir(root, &name, pins) {
            Ok(summary) => index.insert(summary),
            Err(reason) => skipped.push(Skipped { name, reason }),
        }
    }
    Ok((index, skipped))
}

fn summarize_dir(root: &Path, name: &str, pins: &Pins) -> Result<Summary, String> {
    let session = open_session(root, name).map_err(|e| e.to_string())?;
    let meta = session.meta();
    if meta.id != name {
        return Err(format!(
            "metadata names session {:?}, not {name:?}",
            meta.id
        ));
    }
    let size = session.size_bytes().map_err(|e| e.to_string())?;
    Summary::from_meta(meta, size, pins.contains(name))
}
