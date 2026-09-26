//! The sync policy: when a new download of the season is due.
//!
//! The sources publish results and prices at different lags after a
//! session. The policy keeps a scheduled sync cheap: it runs only when
//! something can have changed.

use chrono::Duration;

/// How long after a session ends a sync stays due.
pub const DUE_WINDOW: Duration = Duration::hours(6);

/// The oldest a dataset may be before a sync is due regardless of the
/// calendar; ownership and prices move between weekends.
pub const MAX_AGE: Duration = Duration::hours(24);

/// How long a finished round's points must stand unchanged before they
/// count as final.
pub const STABLE_WINDOW: Duration = Duration::hours(24);

/// How long after a race the sync keeps checking for the finalised points
/// at [`SETTLE_INTERVAL`] instead of the daily refresh.
pub const SETTLE_WINDOW: Duration = Duration::hours(36);

/// The interval between checks for finalised points inside
/// [`SETTLE_WINDOW`].
pub const SETTLE_INTERVAL: Duration = Duration::hours(2);

/// A round whose points were first seen this long after its race counts
/// as final at once.
pub const LATE_SYNC_IS_FINAL: Duration = Duration::days(3);

/// The outcome of a due check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncDecision {
    /// A sync should run now.
    pub due: bool,
    /// The reason, in plain words.
    pub reason: String,
}

impl SyncDecision {
    pub(crate) fn due(reason: impl Into<String>) -> Self {
        Self {
            due: true,
            reason: reason.into(),
        }
    }

    pub(crate) fn not_due(reason: impl Into<String>) -> Self {
        Self {
            due: false,
            reason: reason.into(),
        }
    }
}
