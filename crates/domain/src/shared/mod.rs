//! Value objects shared by every part of the domain.

mod ids;
mod kind;

pub use ids::{AssetId, Gameday, RoundNumber, TeamId, Tla};
pub use kind::AssetKind;
