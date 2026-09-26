//! Data transfer objects: the inputs a client sends and the views a use
//! case returns.
//!
//! The views carry `serde` derives because they are the API contract. The
//! JSON field names match the original Go service, so the same web client
//! works against both.

mod inputs;
mod rules;
mod views;

pub use inputs::{Chip, ConditionsInput, OptimizeInput, ProjectInput, ReviewInput, RiskMode};
pub use rules::RulesDto;
pub use views::{
    AssetProjection, AssetView, BacktestView, ChipValue, HindsightView, HistoryView, OptimizeView,
    PriceBacktestView, PricePredictionView, PricesView, ProjectionView, ReviewAsset, ReviewView,
    RoundResultView, RoundView, SeasonView, SyncView, TeamAsset, TeamView,
};
