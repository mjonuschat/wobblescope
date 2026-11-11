//! Admission: deciding whether a test or import may start, evicting the oldest eligible
//! sessions to make room, and holding the space until the session is stored.

use std::fs;
use std::sync::Arc;

use wobblescope_core::RawSamples;
use wobblescope_session::SessionError;
use wobblescope_session::fs::{open_session, write_session};
use wobblescope_session::meta::{SegmentFile, SessionMeta, validate_session_id};

use super::index::{self, Summary};
use super::{
    Inner, State, Storage, StorageError, leased, remove_and_forget, reserved_total,
    result_reserved_total,
};

/// Space and an id held for a test that has been admitted. Dropping it unused releases both.
pub struct Reservation {
    pub(super) inner: Arc<Inner>,
    pub(super) id: String,
    pub(super) armed: bool,
}

impl Reservation {
    /// The reserved session id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Disarms the guard so its drop never locks, and hands back the id.
    fn into_id(mut self) -> String {
        self.armed = false;
        std::mem::take(&mut self.id)
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if self.armed {
            self.inner.lock().reservations.remove(&self.id);
        }
    }
}

impl Storage {
    /// Asks whether a session `id` of about `estimate_bytes` may start.
    ///
    /// Everything is planned under the lock before anything is deleted: a refusal for a limit
    /// deletes nothing. When the cap would be exceeded, the oldest sessions that are neither
    /// pinned nor leased are evicted, just enough to fit. Free space is the probe's reading
    /// plus what the eviction frees, minus space promised to live reservations and to saves in
    /// progress (which count toward free space only, never toward the cap), and must cover
    /// the estimate plus headroom. An I/O error while evicting stops there; sessions already
    /// evicted stay deleted.
    ///
    /// # Errors
    /// [`StorageError::IdTaken`], [`StorageError::CapExceeded`],
    /// [`StorageError::InsufficientDisk`], or an I/O error.
    pub fn admit(&self, id: &str, estimate_bytes: u64) -> Result<Reservation, StorageError> {
        validate_session_id(id)?;
        let inner = &self.inner;
        let mut st = inner.lock();
        if st.index.get(id).is_some()
            || st.reservations.contains_key(id)
            || fs::symlink_metadata(inner.sessions_dir.join(id)).is_ok()
            || fs::symlink_metadata(inner.sessions_dir.join(format!(".{id}.tmp"))).is_ok()
        {
            return Err(StorageError::IdTaken(id.to_owned()));
        }
        let reserved = reserved_total(&st);
        let used = st.index.used_bytes().saturating_add(reserved);
        let over = used
            .saturating_add(estimate_bytes)
            .saturating_sub(inner.cap_bytes);
        let (plan, freed) = if over > 0 {
            plan_eviction(&st, over, estimate_bytes, inner.cap_bytes)?
        } else {
            (Vec::new(), 0)
        };
        let free = (inner.probe)(&inner.sessions_dir)?;
        let available = free
            .saturating_add(freed)
            .saturating_sub(reserved)
            .saturating_sub(result_reserved_total(&st));
        let need = estimate_bytes.saturating_add(inner.headroom_bytes);
        if available < need {
            return Err(StorageError::InsufficientDisk { need, available });
        }
        for victim in &plan {
            remove_and_forget(inner, &mut st, victim)?;
        }
        st.reservations.insert(id.to_owned(), estimate_bytes);
        Ok(Reservation {
            inner: self.inner.clone(),
            id: id.to_owned(),
            armed: true,
        })
    }

    /// Writes and seals the session a reservation was made for, and indexes it at once.
    ///
    /// The metadata must carry the reserved id and an RFC 3339 `created_at`, both checked
    /// before any file is written. The reservation is released whether or not the write
    /// succeeds. Cap accounting uses the sealed session's actual size, so the cap is soft by
    /// the difference from the estimate until the next admission.
    ///
    /// # Errors
    /// [`StorageError::BadSession`], or the error from writing the session.
    pub fn store<S>(
        &self,
        reservation: Reservation,
        meta: &SessionMeta,
        source: S,
    ) -> Result<Summary, StorageError>
    where
        S: FnMut(&SegmentFile) -> Result<RawSamples, SessionError>,
    {
        let id = reservation.into_id();
        // Writing the session takes as long as the disk takes, so it runs without the state lock:
        // listing and downloading go on meanwhile. The reservation still holds the id and the
        // space, and nothing can see the session until it is indexed below.
        let written = write_sealed(&self.inner, &id, meta, source);
        let mut st = self.inner.lock();
        let result = written.and_then(|()| index_sealed(&self.inner, &mut st, &id, meta));
        st.reservations.remove(&id);
        result
    }
}

/// Checks `meta` against the reservation and writes the sealed session to disk. Takes no lock.
fn write_sealed<S>(
    inner: &Inner,
    id: &str,
    meta: &SessionMeta,
    source: S,
) -> Result<(), StorageError>
where
    S: FnMut(&SegmentFile) -> Result<RawSamples, SessionError>,
{
    if meta.id != id {
        return Err(StorageError::BadSession(format!(
            "metadata names session {:?} but the reservation is for {id:?}",
            meta.id
        )));
    }
    index::created_at_unix(&meta.created_at).map_err(StorageError::BadSession)?;
    write_session(&inner.sessions_dir, meta, source)?;
    Ok(())
}

/// Sizes the sealed session `id`, indexes it and returns its summary.
pub(super) fn index_sealed(
    inner: &Inner,
    st: &mut State,
    id: &str,
    meta: &SessionMeta,
) -> Result<Summary, StorageError> {
    let size = open_session(&inner.sessions_dir, id)?.size_bytes()?;
    let summary =
        Summary::from_meta(meta, size, st.pins.contains(id)).map_err(StorageError::BadSession)?;
    st.index.insert(summary.clone());
    Ok(summary)
}

/// The shortest oldest-first prefix of evictable sessions whose sizes cover `over`, and the
/// bytes it frees.
fn plan_eviction(
    st: &State,
    over: u64,
    estimate: u64,
    cap: u64,
) -> Result<(Vec<String>, u64), StorageError> {
    let mut candidates: Vec<&Summary> = st
        .index
        .iter()
        .filter(|s| !s.pinned && !leased(st, &s.id))
        .collect();
    candidates.sort_by(|a, b| {
        a.created_at_unix
            .cmp(&b.created_at_unix)
            .then_with(|| a.id.cmp(&b.id))
    });
    let evictable = candidates
        .iter()
        .fold(0u64, |sum, s| sum.saturating_add(s.size_bytes));
    if evictable < over {
        return Err(StorageError::CapExceeded {
            need: estimate,
            cap,
            pinned_bytes: st.index.pinned_bytes(),
            evictable_bytes: evictable,
        });
    }
    let mut freed = 0u64;
    let mut ids = Vec::new();
    for s in candidates {
        if freed >= over {
            break;
        }
        freed = freed.saturating_add(s.size_bytes);
        ids.push(s.id.clone());
    }
    Ok((ids, freed))
}
