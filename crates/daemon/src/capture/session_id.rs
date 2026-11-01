//! Session ids: `<prefix>-<UTC compact time>`, with `-2`, `-3` suffixes when the second is taken.

use crate::storage::StorageError;

/// The highest suffix tried before giving up on a second's id.
pub const MAX_SUFFIX: u32 = 99;

/// The id for something created at `created_at` (`YYYY-MM-DDTHH:MM:SSZ`): the prefix and the
/// time without its separators, such as `test-20261101T121400Z`.
#[must_use]
pub fn base_id(prefix: &str, created_at: &str) -> String {
    format!("{prefix}-{}", created_at.replace(['-', ':'], ""))
}

/// Calls `attempt` with `base`, then `base-2`, `base-3` and so on up to [`MAX_SUFFIX`], and
/// returns the first answer that is not [`StorageError::IdTaken`], with the id it was for.
/// `Ok(None)` means every id was taken.
///
/// # Errors
/// Any error other than `IdTaken` from `attempt`.
pub fn first_free_id<T>(
    base: &str,
    mut attempt: impl FnMut(&str) -> Result<T, StorageError>,
) -> Result<Option<(String, T)>, StorageError> {
    for n in 1..=MAX_SUFFIX {
        let id = if n == 1 {
            base.to_owned()
        } else {
            format!("{base}-{n}")
        };
        match attempt(&id) {
            Ok(found) => return Ok(Some((id, found))),
            Err(StorageError::IdTaken(_)) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}
