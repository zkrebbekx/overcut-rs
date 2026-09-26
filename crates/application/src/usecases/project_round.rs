//! Project one round.

use crate::dto::{ProjectInput, ProjectionView};
use crate::AppError;

use super::weekend::{projection_view, round_number, to_domain, with_known_weekend};
use super::AnalysisContext;

/// Simulates one round and returns every asset's points distribution.
pub struct ProjectRound<'a> {
    ctx: &'a AnalysisContext,
}

impl<'a> ProjectRound<'a> {
    /// Binds the use case to a context.
    pub fn new(ctx: &'a AnalysisContext) -> Self {
        Self { ctx }
    }

    /// Runs the projection.
    pub fn execute(&self, input: ProjectInput) -> Result<ProjectionView, AppError> {
        let season = self.ctx.season();
        let target = season.resolve_round(round_number(input.round))?;
        let (cond, known) = with_known_weekend(target, input.conditions);
        let domain_cond = to_domain(&cond)?;
        let sim = self.ctx.simulate(
            target,
            self.ctx.settings(input.sims, input.seed),
            &cond,
            &domain_cond,
        );
        Ok(projection_view(&season, target, &sim, cond, known))
    }
}
