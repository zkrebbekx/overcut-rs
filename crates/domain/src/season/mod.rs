//! The `Season` aggregate: the calendar, every classification, and every
//! asset's per-gameday market snapshot.
//!
//! The aggregate is the one thing the toolkit persists. Every analysis
//! reads it; only a sync writes it. The sync policy that decides when a
//! new sync is due lives in [`policy`].

mod aggregate;
mod asset;
pub mod policy;
mod repository;
mod round;

pub use aggregate::Season;
pub use asset::{Asset, ComponentStats, GamedaySnapshot};
pub use repository::{RepositoryError, SeasonRepository};
pub use round::{RaceRow, Round, Session, SprintOrder};
