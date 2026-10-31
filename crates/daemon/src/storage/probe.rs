//! The default free-space probe.

use std::io;
use std::path::Path;

/// Bytes available to unprivileged users on the filesystem holding `path`.
///
/// # Errors
/// The I/O error from `statvfs`.
pub fn available_bytes(path: &Path) -> io::Result<u64> {
    let st = rustix::fs::statvfs(path)?;
    Ok(st.f_bavail.saturating_mul(st.f_frsize))
}
