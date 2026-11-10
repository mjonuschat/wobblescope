//! Who may use the daemon: the policy mirrored from Moonraker's `[authorization]` section.

pub mod client;
pub mod names;
pub mod policy;
pub mod watcher;

use std::sync::Arc;

pub use policy::{Ignored, IgnoredList, Net, Policy, Unreadable};

/// What the supervisor publishes about access.
#[derive(Debug, Clone, PartialEq)]
pub enum AccessState {
    /// Moonraker's lists are read.
    Known(Arc<Policy>),
    /// The lists are not read: Moonraker is unreachable or its answer lacks them.
    Unknown {
        /// Whether the last `login_required` read said true. It stays until a read says
        /// otherwise, so a Moonraker restart does not lift it.
        logins_required: bool,
    },
}

impl Default for AccessState {
    fn default() -> Self {
        Self::Unknown {
            logins_required: false,
        }
    }
}
