//! The official F1 Fantasy scoring rules for 2026.
//!
//! [`ScoringRules`] is a value object that holds every scoring table and
//! constant for one season. [`ScoringRules::default`] returns the 2026
//! tables. An adapter can build a modified value from a file to track a
//! mid-season rule change without a code change.
//!
//! The scoring functions live on the value object. They are pure: the same
//! [`DriverWeekend`] always scores the same [`Breakdown`].

mod scoring;
mod tables;
mod weekend;

pub use scoring::ConstructorScore;
pub use tables::{PitStopBand, QualiBonus, ScoringRules, SessionPenalty};
pub use weekend::{Breakdown, DriverWeekend};
