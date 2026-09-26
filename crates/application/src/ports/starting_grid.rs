//! The official starting grid, with penalties applied.

use async_trait::async_trait;
use overcut_domain::shared::Tla;

use super::GatewayError;

/// One car on the starting grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridSlot {
    /// The driver.
    pub tla: Tla,
    /// The grid position from P1.
    pub position: u32,
}

/// Reads the official starting grid of a race.
#[async_trait]
pub trait StartingGridGateway: Send + Sync {
    /// Fetches the grid for the race with the given name in the season, or
    /// `Ok(None)` when the grid is not published or the race is unknown.
    async fn starting_grid(
        &self,
        season: u16,
        race_name: &str,
    ) -> Result<Option<Vec<GridSlot>>, GatewayError>;
}
