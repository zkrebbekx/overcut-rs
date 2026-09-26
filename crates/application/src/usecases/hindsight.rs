//! The best team for a finished round.

use std::collections::HashSet;

use overcut_domain::optimizer::{best_lineups, Candidate, OptimizerOptions, TeamShape};
use overcut_domain::season::{Round, Season};

use crate::dto::HindsightView;
use crate::AppError;

use super::optimize_team::team_view;
use super::weekend::round_number;
use super::AnalysisContext;

/// Enumerates the best teams for a finished round by official points.
pub struct Hindsight<'a> {
    ctx: &'a AnalysisContext,
}

impl<'a> Hindsight<'a> {
    /// Binds the use case to a context.
    pub fn new(ctx: &'a AnalysisContext) -> Self {
        Self { ctx }
    }

    /// Returns the `top` best teams. A zero round means the latest finished
    /// round; a zero `top` means five.
    pub fn execute(&self, round: u32, top: usize) -> Result<HindsightView, AppError> {
        let season = self.ctx.season();
        let target = season.resolve_finished_round(round_number(round))?;
        Ok(self.for_round(&season, target, top))
    }

    /// Enumerates the best teams for an already-resolved finished round.
    pub(crate) fn for_round(&self, season: &Season, target: &Round, top: usize) -> HindsightView {
        let top = if top == 0 { 5 } else { top };
        let gameday = target.number.gameday();
        let candidates: Vec<Candidate> = season
            .assets()
            .iter()
            .filter_map(|a| {
                let h = a.at(gameday)?;
                Some(Candidate {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    kind: a.kind,
                    price: h.price,
                    points: h.points,
                })
            })
            .collect();
        let rules = self.ctx.rules();
        let shape = TeamShape {
            drivers: rules.team_drivers,
            constructors: rules.team_constructors,
        };
        let teams = best_lineups(
            &candidates,
            &OptimizerOptions::new(rules.budget, shape).top(top),
        );
        HindsightView {
            round: target.number.get(),
            name: target.name.clone(),
            teams: teams
                .iter()
                .map(|t| team_view(t, &HashSet::new()))
                .collect(),
        }
    }
}
