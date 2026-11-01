//! Storing a capture: a session written segment by segment while the test runs, then sealed and
//! indexed like any other.
//!
//! A capture's directory is `.<id>.tmp`, so it is not listed, not evictable and not deletable
//! until it is sealed. Its reservation holds the estimated bytes the whole time.

use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::Coverage;
use wobblescope_session::meta::{SegmentFile, SessionMeta};
use wobblescope_session::writer::SessionWriter;

use super::admission::index_sealed;
use super::index::{self, Summary};
use super::{Reservation, Storage, StorageError};

/// A capture being written. Dropping it releases the reservation and leaves the tmp directory
/// for the next start's recovery.
pub struct Capture {
    writer: SessionWriter,
    reservation: Reservation,
}

impl Capture {
    /// The session id.
    #[must_use]
    pub fn id(&self) -> &str {
        self.writer.id()
    }

    /// Writes the files of segment `index`, synced, and returns their metadata entries. Blocks
    /// on the disk.
    ///
    /// # Errors
    /// A session error from encoding or writing.
    pub fn write_segment(
        &mut self,
        index: usize,
        recordings: &[(u32, &RawSamples, Coverage)],
    ) -> Result<Vec<SegmentFile>, StorageError> {
        Ok(self.writer.write_segment(index, recordings)?)
    }

    /// Rewrites the journal to `meta`. Blocks on the disk.
    ///
    /// # Errors
    /// A session error from validating or writing.
    pub fn journal(&self, meta: &SessionMeta) -> Result<(), StorageError> {
        Ok(self.writer.journal(meta)?)
    }
}

impl Storage {
    /// Starts a capture for the session `meta` describes, with the space `reservation` holds. The
    /// metadata should list no segment yet; it becomes the first journal.
    ///
    /// # Errors
    /// [`StorageError::BadSession`] when the metadata and the reservation name different
    /// sessions, or a session error from creating the directory.
    pub fn begin_capture(
        &self,
        reservation: Reservation,
        meta: &SessionMeta,
    ) -> Result<Capture, StorageError> {
        if meta.id != reservation.id() {
            return Err(StorageError::BadSession(format!(
                "metadata names session {:?} but the reservation is for {:?}",
                meta.id,
                reservation.id()
            )));
        }
        let writer = SessionWriter::create(&self.inner.sessions_dir, &meta.id, meta)?;
        Ok(Capture {
            writer,
            reservation,
        })
    }

    /// Seals the capture with its final metadata, sizes and indexes the session and releases
    /// the reservation. On failure the directory and its journal stay as they are, so the caller
    /// can [`Storage::abandon_capture`] it or drop it for the next start's recovery, and the
    /// reservation is still held until the capture is dropped.
    ///
    /// # Errors
    /// [`StorageError::BadSession`] for a bad `created_at`, or a session error from sealing.
    pub fn seal_capture(
        &self,
        capture: &Capture,
        meta: &SessionMeta,
    ) -> Result<Summary, StorageError> {
        let id = capture.id();
        let mut st = self.inner.lock();
        index::created_at_unix(&meta.created_at).map_err(StorageError::BadSession)?;
        capture.writer.seal(meta)?;
        st.reservations.remove(id);
        index_sealed(&self.inner, &mut st, id, meta)
    }

    /// Removes the capture's directory and releases its reservation.
    ///
    /// # Errors
    /// A session error when the directory cannot be removed. The reservation is released
    /// either way.
    pub fn abandon_capture(&self, capture: Capture) -> Result<(), StorageError> {
        let Capture {
            writer,
            reservation,
        } = capture;
        let result = writer.abandon();
        drop(reservation);
        Ok(result?)
    }
}
