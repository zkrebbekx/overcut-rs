//! The persistence port of the `Season` aggregate.

use thiserror::Error;

use crate::season::Season;

/// A failure of the season store. The adapter wraps its own error.
#[derive(Debug, Error)]
#[error("season store: {0}")]
pub struct RepositoryError(#[from] pub Box<dyn std::error::Error + Send + Sync + 'static>);

impl RepositoryError {
    /// Wraps any error.
    pub fn new(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self(Box::new(err))
    }

    /// Builds an error from a message.
    pub fn message(msg: impl Into<String>) -> Self {
        Self(msg.into().into())
    }
}

/// Stores and loads the one season the toolkit works on.
///
/// The domain defines the port; an infrastructure adapter implements it.
/// The default adapter keeps the season in one JSON file.
pub trait SeasonRepository: Send + Sync {
    /// Loads the season, or `Ok(None)` when no season is stored yet.
    fn load(&self) -> Result<Option<Season>, RepositoryError>;

    /// Stores the season, replacing any previous one.
    fn save(&self, season: &Season) -> Result<(), RepositoryError>;
}
