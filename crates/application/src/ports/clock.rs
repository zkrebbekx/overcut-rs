//! The clock port.

use chrono::{DateTime, Utc};

/// Supplies the current time. Tests inject a fixed clock.
pub trait Clock: Send + Sync {
    /// The current instant in UTC.
    fn now(&self) -> DateTime<Utc>;
}
