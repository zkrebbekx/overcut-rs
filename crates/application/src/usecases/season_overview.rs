//! The season shaped for a client.

use chrono::{DateTime, Utc};
use overcut_domain::shared::Gameday;

use crate::dto::{AssetView, HistoryView, RoundView, SeasonView};

use super::AnalysisContext;

/// Returns the calendar and every asset with its history.
pub struct SeasonOverview<'a> {
    ctx: &'a AnalysisContext,
}

impl<'a> SeasonOverview<'a> {
    /// Binds the use case to a context.
    pub fn new(ctx: &'a AnalysisContext) -> Self {
        Self { ctx }
    }

    /// Builds the view. `now` decides which rounds are provisional.
    pub fn execute(&self, now: DateTime<Utc>) -> SeasonView {
        let season = self.ctx.season();
        let rounds = season
            .rounds()
            .iter()
            .map(|r| RoundView {
                round: r.number.get(),
                name: r.name.clone(),
                circuit_id: r.circuit_id.clone(),
                date: r.date.map(|d| d.to_string()).unwrap_or_default(),
                has_sprint: r.has_sprint,
                has_results: r.has_results,
                has_quali: r.has_quali(),
                has_grid: r.grid_order().is_some(),
                has_sprint_result: r.has_sprint_result(),
                sessions: r
                    .sessions
                    .iter()
                    .map(|(s, t)| (s.as_str().to_string(), *t))
                    .collect(),
                provisional: r.provisional(now),
            })
            .collect();
        let assets = season
            .assets()
            .iter()
            .filter_map(|a| {
                let latest = a.latest()?;
                Some(AssetView {
                    id: a.id.to_string(),
                    kind: a.kind.as_str().to_string(),
                    name: a.name.clone(),
                    tla: a.tla.as_ref().map(ToString::to_string).unwrap_or_default(),
                    team_id: a.team_id.to_string(),
                    team_name: a.team_label().to_string(),
                    price: latest.price,
                    old_price: latest.old_price,
                    ownership: latest.ownership,
                    total_points: a.history.iter().map(|h| h.points).sum(),
                    selectable: season.selectable(a),
                    history: a
                        .history
                        .iter()
                        .map(|h| HistoryView {
                            gameday: h.gameday.get(),
                            price: h.price,
                            points: h.points,
                            quali_pts: h.quali_pts,
                            sprint_pts: h.sprint_pts,
                            race_pts: h.race_pts,
                            ownership: h.ownership,
                        })
                        .collect(),
                })
            })
            .collect();
        SeasonView {
            season: season.year(),
            synced_at: season.synced_at(),
            latest_gameday: season.latest_gameday().map_or(0, Gameday::get),
            next_round: season.next_round().map_or(0, |r| r.number.get()),
            rounds,
            assets,
        }
    }
}
