//! Domain errors.

use thiserror::Error;

/// An error raised by the domain when an invariant does not hold.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// A three-letter driver code was empty or not alphanumeric.
    #[error("invalid three-letter code {0:?}: expected 1 to 4 ASCII letters or digits")]
    InvalidTla(String),

    /// An identifier was empty.
    #[error("empty {0} identifier")]
    EmptyId(&'static str),

    /// A round number was zero.
    #[error("round number must be at least 1")]
    ZeroRound,

    /// A gameday number was zero.
    #[error("gameday number must be at least 1")]
    ZeroGameday,

    /// Two rounds share a number.
    #[error("duplicate round {0}")]
    DuplicateRound(u32),

    /// Two assets share an identifier.
    #[error("duplicate asset {0:?}")]
    DuplicateAsset(String),

    /// The caller asked for a round that the calendar does not hold.
    #[error("round {0} is not on the calendar")]
    UnknownRound(u32),

    /// The caller asked for the next round but the season is complete.
    #[error("the season is complete; pass a round")]
    SeasonComplete,

    /// The caller asked for a finished round but none exists.
    #[error("no completed rounds")]
    NoCompletedRounds,

    /// The caller asked for a round that has no race result yet.
    #[error("round {0} has no results")]
    RoundNotFinished(u32),

    /// The optimizer could not build one legal team.
    #[error("no legal team fits the budget")]
    NoLegalTeam,

    /// The caller named a chip the game does not have.
    #[error("unknown chip {0:?}")]
    UnknownChip(String),

    /// The caller named a risk mode the optimizer does not have.
    #[error("unknown risk mode {0:?}: expected mean, p10, or p90")]
    UnknownRiskMode(String),

    /// The caller named an asset that the season does not hold.
    #[error("unknown asset {0:?} (use a driver code like VER or a constructor name prefix like McLaren)")]
    UnknownAsset(String),
}
