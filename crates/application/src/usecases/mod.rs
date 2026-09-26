//! The use cases.
//!
//! Every analysis use case borrows an [`AnalysisContext`], which holds the
//! season, the rules, and the caches of expensive results. The sync use
//! case owns its gateways and replaces the season in the context when it
//! finishes.

pub mod context;
mod hindsight;
mod optimize_team;
mod predict_prices;
mod project_round;
mod review_round;
mod run_backtest;
mod season_overview;
mod sync_season;
mod team;
mod weekend;

pub use context::AnalysisContext;
pub use hindsight::Hindsight;
pub use optimize_team::OptimizeTeam;
pub use predict_prices::PredictPrices;
pub use project_round::ProjectRound;
pub use review_round::ReviewRound;
pub use run_backtest::RunBacktest;
pub use season_overview::SeasonOverview;
pub use sync_season::{SyncSeason, SyncSources};
pub use team::resolve_team;

/// The default simulation count for a projection.
pub const DEFAULT_SIMS: usize = 20_000;

/// The default random seed.
pub const DEFAULT_SEED: u64 = 1;

/// The default simulation count per round of the backtest.
pub const DEFAULT_BACKTEST_SIMS: usize = 3_000;
