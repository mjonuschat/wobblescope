//! One way to take a mutex whose holder may have panicked.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locks `mutex`, taking the data even when a holder panicked. The state behind every mutex in
/// the daemon is a plain snapshot that stays valid after a panic, so a poisoned lock is not an
/// error.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
