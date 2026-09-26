//! The ports through which the application reaches the outside world.
//!
//! Every port is a trait. The application owns the trait and the record
//! types it exchanges; an adapter in the infrastructure crate implements
//! the trait against a real source. The records are source-agnostic: a
//! quirk of one provider (a string-encoded number, a mislabelled session)
//! is resolved inside the adapter and never reaches this layer.

mod clock;
mod fantasy_feed;
mod race_data;
mod starting_grid;

use thiserror::Error;

pub use clock::Clock;
pub use fantasy_feed::{FantasyFeedGateway, FeedGameday, FeedPlayer};
pub use race_data::{
    CalendarEntry, ClassifiedCar, QualifyingClassification, RaceClassification, RaceDataGateway,
};
pub use starting_grid::{GridSlot, StartingGridGateway};

/// A failure of an external data source.
#[derive(Debug, Error)]
#[error("{gateway}: {message}")]
pub struct GatewayError {
    /// The source that failed.
    pub gateway: &'static str,
    /// What went wrong.
    pub message: String,
}

impl GatewayError {
    /// Builds an error for one gateway.
    pub fn new(gateway: &'static str, message: impl Into<String>) -> Self {
        Self {
            gateway,
            message: message.into(),
        }
    }
}
