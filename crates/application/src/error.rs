//! Application errors.

use overcut_domain::season::RepositoryError;
use overcut_domain::DomainError;
use thiserror::Error;

use crate::ports::GatewayError;

/// An error raised by a use case.
#[derive(Debug, Error)]
pub enum AppError {
    /// A domain invariant or lookup failed. The request was wrong.
    #[error(transparent)]
    Domain(#[from] DomainError),

    /// The caller's input was malformed.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// The season store failed.
    #[error(transparent)]
    Repository(#[from] RepositoryError),

    /// An external data source failed.
    #[error(transparent)]
    Gateway(#[from] GatewayError),

    /// A sync produced a season without assets.
    #[error("the fantasy feed returned no assets")]
    NoAssets,
}

impl AppError {
    /// Reports whether the error is the caller's fault.
    pub fn is_client_error(&self) -> bool {
        matches!(self, Self::Domain(_) | Self::InvalidInput(_))
    }
}
