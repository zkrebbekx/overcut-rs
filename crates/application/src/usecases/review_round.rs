//! Compare a finished round's grid-known projection with the official
//! points.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use overcut_domain::shared::{AssetId, AssetKind};

use crate::dto::{ConditionsInput, ReviewAsset, ReviewInput, ReviewView};
use crate::AppError;

use super::hindsight::Hindsight;
use super::weekend::{round_number, to_domain, with_known_weekend};
use super::AnalysisContext;

/// Reviews a finished round: what the model expected on Sunday morning
/// against what happened.
pub struct ReviewRound<'a> {
    ctx: &'a AnalysisContext,
}

impl<'a> ReviewRound<'a> {
    /// Binds the use case to a context.
    pub fn new(ctx: &'a AnalysisContext) -> Self {
        Self { ctx }
    }

    /// Runs the review. `now` decides whether the points are provisional.
    pub fn execute(&self, input: ReviewInput, now: DateTime<Utc>) -> Result<ReviewView, AppError> {
        let season = self.ctx.season();
        let target = season.resolve_finished_round(round_number(input.round))?;
        let (cond, _) = with_known_weekend(target, ConditionsInput::default());
        let domain_cond = to_domain(&cond)?;
        let sim = self.ctx.simulate(
            target,
            self.ctx.settings(input.sims, input.seed),
            &cond,
            &domain_cond,
        );

        let held: HashSet<AssetId> = input
            .team
            .into_iter()
            .filter_map(|id| AssetId::new(id).ok())
            .collect();
        let captain = AssetId::new(input.captain).ok();
        let gameday = target.number.gameday();
        let mut view = ReviewView {
            round: target.number.get(),
            name: target.name.clone(),
            has_sprint: target.has_sprint,
            sims: sim.sims,
            assets: Vec::new(),
            coverage: 0.0,
            driver_mae: 0.0,
            provisional: target.provisional(now),
            team_projected: 0.0,
            team_actual: 0.0,
            captain_id: String::new(),
            hindsight_points: 0.0,
        };
        let (mut drivers, mut in_range) = (0.0, 0.0);
        let mut best_held = f64::NEG_INFINITY;
        for a in season.assets() {
            let (Some(h), Some(p)) = (a.at(gameday), sim.by_id(&a.id)) else {
                continue;
            };
            let delta = h.points - p.dist.mean;
            let is_held = held.contains(&a.id);
            let ra = ReviewAsset {
                id: a.id.to_string(),
                name: a.name.clone(),
                kind: a.kind.as_str().to_string(),
                tla: a.tla.as_ref().map(ToString::to_string).unwrap_or_default(),
                team_name: a.team_label().to_string(),
                price: h.price,
                ownership: h.ownership,
                projected: p.dist.mean,
                sd: p.dist.sd,
                p10: p.dist.p10,
                p90: p.dist.p90,
                actual: h.points,
                delta,
                z: if p.dist.sd > 0.0 {
                    delta / p.dist.sd
                } else {
                    0.0
                },
                in_range: h.points >= p.dist.p10 && h.points <= p.dist.p90,
                held: is_held,
            };
            if a.kind == AssetKind::Driver {
                drivers += 1.0;
                view.driver_mae += delta.abs();
                if ra.in_range {
                    in_range += 1.0;
                }
                let is_captain = captain.as_ref() == Some(&a.id);
                let auto_captain = captain.is_none() && p.dist.mean > best_held;
                if is_held && (is_captain || auto_captain) {
                    if auto_captain {
                        best_held = p.dist.mean;
                    }
                    view.captain_id = a.id.to_string();
                }
            }
            if is_held {
                view.team_projected += p.dist.mean;
                view.team_actual += h.points;
            }
            view.assets.push(ra);
        }
        if drivers > 0.0 {
            view.coverage = in_range / drivers;
            view.driver_mae /= drivers;
        }
        if !view.captain_id.is_empty() {
            if let Some(c) = view.assets.iter().find(|ra| ra.id == view.captain_id) {
                view.team_projected += c.projected;
                view.team_actual += c.actual;
            }
        }
        view.assets
            .sort_by(|a, b| b.delta.abs().total_cmp(&a.delta.abs()));
        view.hindsight_points = Hindsight::new(self.ctx)
            .for_round(&season, target, 1)
            .teams
            .first()
            .map_or(0.0, |t| t.score);
        Ok(view)
    }
}
