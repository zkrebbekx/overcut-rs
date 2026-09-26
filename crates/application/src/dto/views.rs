//! Use case outputs.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::ConditionsInput;

/// The season shaped for a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeasonView {
    /// The season year.
    pub season: u16,
    /// When the data was downloaded.
    pub synced_at: DateTime<Utc>,
    /// The highest gameday in the data.
    pub latest_gameday: u32,
    /// The next round without a result, or zero when the season is done.
    pub next_round: u32,
    /// The calendar.
    pub rounds: Vec<RoundView>,
    /// Every asset with its history.
    pub assets: Vec<AssetView>,
}

/// One calendar entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoundView {
    /// The round number.
    pub round: u32,
    /// The race name.
    pub name: String,
    /// The circuit identifier.
    pub circuit_id: String,
    /// The race date, ISO formatted.
    pub date: String,
    /// The weekend has a sprint.
    pub has_sprint: bool,
    /// The race classification is in.
    pub has_results: bool,
    /// The official qualifying classification is in.
    pub has_quali: bool,
    /// The official starting grid is in.
    pub has_grid: bool,
    /// The sprint classification is in.
    pub has_sprint_result: bool,
    /// The session start times in UTC, keyed by session label.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sessions: BTreeMap<String, DateTime<Utc>>,
    /// The round's official points may still change.
    pub provisional: bool,
}

/// One asset with its history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetView {
    /// The identifier.
    pub id: String,
    /// "driver" or "constructor".
    pub kind: String,
    /// The display name.
    pub name: String,
    /// The driver code.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tla: String,
    /// The team identifier.
    pub team_id: String,
    /// The team name.
    pub team_name: String,
    /// The current price.
    pub price: f64,
    /// The previous price.
    pub old_price: f64,
    /// The current ownership in percent.
    pub ownership: f64,
    /// The season points to date.
    pub total_points: f64,
    /// The asset can join a team now.
    pub selectable: bool,
    /// One entry per gameday.
    pub history: Vec<HistoryView>,
}

/// One gameday of an asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryView {
    /// The gameday.
    pub gameday: u32,
    /// The price.
    pub price: f64,
    /// The official points.
    pub points: f64,
    /// The qualifying points.
    pub quali_pts: f64,
    /// The sprint points.
    pub sprint_pts: f64,
    /// The race points.
    pub race_pts: f64,
    /// The ownership in percent.
    pub ownership: f64,
}

/// One round's projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionView {
    /// The round.
    pub round: u32,
    /// The race name.
    pub name: String,
    /// The weekend has a sprint.
    pub has_sprint: bool,
    /// The simulation count.
    pub sims: usize,
    /// The state the simulation used, including any order filled in from
    /// the official data.
    pub conditions: ConditionsInput,
    /// The qualifying order came from the official data.
    pub quali_from_data: bool,
    /// The grid came from the official data.
    pub grid_from_data: bool,
    /// The sprint result came from the official data.
    pub sprint_from_data: bool,
    /// One projection per asset, highest mean first.
    pub assets: Vec<AssetProjection>,
}

/// One asset's projected distribution plus market data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetProjection {
    /// The identifier.
    pub id: String,
    /// The display name.
    pub name: String,
    /// "driver" or "constructor".
    pub kind: String,
    /// The driver code.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tla: String,
    /// The team identifier.
    pub team_id: String,
    /// The team name.
    pub team_name: String,
    /// The current price.
    pub price: f64,
    /// The ownership in percent.
    pub ownership: f64,
    /// The mean projected points.
    pub mean: f64,
    /// The standard deviation.
    pub sd: f64,
    /// The 10th percentile.
    pub p10: f64,
    /// The median.
    pub p50: f64,
    /// The 90th percentile.
    pub p90: f64,
    /// The previous round's points.
    pub last_points: f64,
    /// The mean points over the rounds before this one.
    pub avg_points: f64,
    /// The official score once the round is complete.
    pub actual_points: f64,
    /// `actual_points` is set.
    pub has_actual: bool,
}

/// One asset inside a team.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamAsset {
    /// The identifier.
    pub id: String,
    /// The display name.
    pub name: String,
    /// "driver" or "constructor".
    pub kind: String,
    /// The price.
    pub price: f64,
    /// The projected (or actual) points the optimizer used.
    pub points: f64,
}

/// One optimized team.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamView {
    /// The drivers.
    pub drivers: Vec<TeamAsset>,
    /// The constructors.
    pub constructors: Vec<TeamAsset>,
    /// The driver with the highest multiplier.
    pub captain_id: String,
    /// The driver with the regular Boost when the x3 chip is played.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub boost_id: String,
    /// The total price.
    pub cost: f64,
    /// The points without boosts or penalty.
    pub raw_points: f64,
    /// The extra points from every boost.
    pub captain_points: f64,
    /// The changes from the current team.
    pub transfers: u32,
    /// The transfer penalty.
    pub penalty: f64,
    /// The optimizer score.
    pub score: f64,
    /// The assets bought.
    #[serde(rename = "in")]
    pub bought: Vec<String>,
    /// The assets sold.
    #[serde(rename = "out")]
    pub sold: Vec<String>,
    /// The team's 10th percentile from the joint simulation, boosts and
    /// penalty included.
    pub p10: f64,
    /// The team's median.
    pub p50: f64,
    /// The team's 90th percentile.
    pub p90: f64,
}

/// The expected gain of one chip on the recommended team.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChipValue {
    /// The chip label.
    pub chip: String,
    /// The display label.
    pub label: String,
    /// The expected gain in points.
    pub gain: f64,
    /// The chip can be valued with the given input.
    pub available: bool,
    /// A short explanation.
    pub note: String,
    /// Final Fix: the driver to swap out.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub out_id: String,
    /// Final Fix: the driver to swap in.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub in_id: String,
}

/// The optimizer response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizeView {
    /// The round.
    pub round: u32,
    /// The race name.
    pub name: String,
    /// The risk mode used.
    pub risk: String,
    /// The chip played, or empty.
    pub chip: String,
    /// The budget used.
    pub budget: f64,
    /// The best teams, best first.
    pub teams: Vec<TeamView>,
    /// The projected score of the current team as-is.
    pub current_score: f64,
    /// The projection the optimizer used.
    pub projection: ProjectionView,
    /// Every chip valued against the best team without a chip.
    pub chips: Vec<ChipValue>,
}

/// One asset's projection against its actual score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewAsset {
    /// The identifier.
    pub id: String,
    /// The display name.
    pub name: String,
    /// "driver" or "constructor".
    pub kind: String,
    /// The driver code.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tla: String,
    /// The team name.
    pub team_name: String,
    /// The price at the round.
    pub price: f64,
    /// The ownership at the round.
    pub ownership: f64,
    /// The projected mean.
    pub projected: f64,
    /// The projected standard deviation.
    pub sd: f64,
    /// The projected 10th percentile.
    pub p10: f64,
    /// The projected 90th percentile.
    pub p90: f64,
    /// The official points.
    pub actual: f64,
    /// Actual minus projected.
    pub delta: f64,
    /// The delta in standard deviations.
    pub z: f64,
    /// The actual fell inside P10–P90.
    pub in_range: bool,
    /// The asset was on the reviewed team.
    pub held: bool,
}

/// The post-round review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewView {
    /// The round.
    pub round: u32,
    /// The race name.
    pub name: String,
    /// The weekend had a sprint.
    pub has_sprint: bool,
    /// The simulation count.
    pub sims: usize,
    /// Every asset, largest surprise first.
    pub assets: Vec<ReviewAsset>,
    /// The share of drivers inside P10–P90.
    pub coverage: f64,
    /// The mean absolute driver error.
    pub driver_mae: f64,
    /// The official points may still be revised.
    pub provisional: bool,
    /// The reviewed team's projected score.
    pub team_projected: f64,
    /// The reviewed team's actual score.
    pub team_actual: f64,
    /// The driver that carried the Boost.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub captain_id: String,
    /// The best team that was possible at the round's prices.
    pub hindsight_points: f64,
}

/// One predicted price change. The JSON names are the Go struct field
/// names, because the Go type carries no tags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricePredictionView {
    /// The identifier.
    #[serde(rename = "AssetID")]
    pub asset_id: String,
    /// The display name.
    #[serde(rename = "Name")]
    pub name: String,
    /// "driver" or "constructor".
    #[serde(rename = "Kind")]
    pub kind: String,
    /// The current price.
    #[serde(rename = "Price")]
    pub price: f64,
    /// The predicted change in millions.
    #[serde(rename = "Change")]
    pub change: f64,
}

/// The walk-forward accuracy of the price predictor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceBacktestView {
    /// The count of predicted movements.
    #[serde(rename = "Examples")]
    pub examples: usize,
    /// The mean absolute error in millions.
    #[serde(rename = "MAE")]
    pub mae: f64,
    /// The error of the always-zero prediction.
    #[serde(rename = "NaiveMAE")]
    pub naive_mae: f64,
    /// The sign hit rate on the moves that happened.
    #[serde(rename = "Direction")]
    pub direction: f64,
    /// The count of nonzero actual moves.
    #[serde(rename = "Moves")]
    pub moves: usize,
}

/// The price predictor response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricesView {
    /// The measured accuracy.
    pub report: PriceBacktestView,
    /// The predictions, largest rise first.
    pub predictions: Vec<PricePredictionView>,
}

/// The accuracy of one projected round.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct RoundResultView {
    #[serde(rename = "Round")]
    pub round: u32,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "DriverMAE")]
    pub driver_mae: f64,
    #[serde(rename = "ConsMAE")]
    pub cons_mae: f64,
    #[serde(rename = "SpearmanRho")]
    pub spearman_rho: f64,
    #[serde(rename = "GridDriverMAE")]
    pub grid_driver_mae: f64,
    #[serde(rename = "GridSpearmanRho")]
    pub grid_spearman_rho: f64,
    #[serde(rename = "GridTeamPts")]
    pub grid_team_pts: f64,
    #[serde(rename = "Coverage")]
    pub coverage: f64,
    #[serde(rename = "GridCoverage")]
    pub grid_coverage: f64,
    #[serde(rename = "ModelTeamPts")]
    pub model_team_pts: f64,
    #[serde(rename = "NaiveTeamPts")]
    pub naive_team_pts: f64,
    #[serde(rename = "HindsightTeamPts")]
    pub hindsight_team_pts: f64,
}

/// The pooled backtest. The JSON names are the Go struct field names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct BacktestView {
    #[serde(rename = "Rounds")]
    pub rounds: Vec<RoundResultView>,
    #[serde(rename = "DriverMAE")]
    pub driver_mae: f64,
    #[serde(rename = "ConsMAE")]
    pub cons_mae: f64,
    #[serde(rename = "MeanSpearman")]
    pub mean_spearman: f64,
    #[serde(rename = "BaselinePrev")]
    pub baseline_prev: f64,
    #[serde(rename = "BaselineSeason")]
    pub baseline_season: f64,
    #[serde(rename = "GridDriverMAE")]
    pub grid_driver_mae: f64,
    #[serde(rename = "GridMeanSpearman")]
    pub grid_mean_spearman: f64,
    #[serde(rename = "GridTeamPts")]
    pub grid_team_pts: f64,
    #[serde(rename = "Coverage")]
    pub coverage: f64,
    #[serde(rename = "GridCoverage")]
    pub grid_coverage: f64,
    #[serde(rename = "ModelTeamPts")]
    pub model_team_pts: f64,
    #[serde(rename = "NaiveTeamPts")]
    pub naive_team_pts: f64,
    #[serde(rename = "HindsightTeamPts")]
    pub hindsight_team_pts: f64,
}

/// The best teams for a finished round.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HindsightView {
    /// The round.
    pub round: u32,
    /// The race name.
    pub name: String,
    /// The best teams by official points, best first.
    pub teams: Vec<TeamView>,
}

/// The outcome of a sync.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncView {
    /// The sync ran. False when `when_due` was set and nothing was due.
    pub synced: bool,
    /// Why the sync ran or was skipped.
    pub reason: String,
    /// The round count.
    pub rounds: usize,
    /// The finished round count.
    pub completed_rounds: usize,
    /// The asset count.
    pub assets: usize,
    /// The rounds whose points are provisional.
    pub provisional_rounds: Vec<u32>,
    /// The next scheduled session, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_session: Option<String>,
}
