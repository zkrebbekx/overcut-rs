//! The fantasy game's public feed: prices, ownership, and official points.

use async_trait::async_trait;
use overcut_domain::season::ComponentStats;
use overcut_domain::shared::AssetKind;

use super::GatewayError;

/// One asset in one gameday document.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedPlayer {
    /// The game's asset identifier.
    pub id: String,
    /// Driver or constructor.
    pub kind: AssetKind,
    /// The display name.
    pub name: String,
    /// The driver code, when the feed carries one.
    pub tla: Option<String>,
    /// The team identifier.
    pub team_id: String,
    /// The team name. Empty on some constructor rows.
    pub team_name: String,
    /// The asset can join a team this gameday.
    pub active: bool,
    /// The price in millions.
    pub price: f64,
    /// The price before this gameday's change.
    pub old_price: f64,
    /// The ownership share in percent.
    pub ownership: f64,
    /// The official points for the gameday.
    pub points: f64,
    /// The official qualifying points.
    pub quali_pts: f64,
    /// The official sprint points (sprint race; sprint qualifying scores
    /// nothing).
    pub sprint_pts: f64,
    /// The official race points.
    pub race_pts: f64,
    /// The cumulative component stats.
    pub stats: ComponentStats,
}

/// One parsed gameday document.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedGameday {
    /// The gameday number.
    pub gameday: u32,
    /// Every asset.
    pub players: Vec<FeedPlayer>,
}

/// Reads gameday documents from the fantasy feed.
#[async_trait]
pub trait FantasyFeedGateway: Send + Sync {
    /// Fetches one gameday, or `Ok(None)` when the game has not published
    /// it yet.
    async fn gameday(&self, gameday: u32) -> Result<Option<FeedGameday>, GatewayError>;
}
