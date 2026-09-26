//! Measure the projection model on past rounds.

use crate::dto::{BacktestView, RoundResultView};

use super::AnalysisContext;

/// Runs the walk-forward backtest.
pub struct RunBacktest<'a> {
    ctx: &'a AnalysisContext,
}

impl<'a> RunBacktest<'a> {
    /// Binds the use case to a context.
    pub fn new(ctx: &'a AnalysisContext) -> Self {
        Self { ctx }
    }

    /// Runs the backtest with `sims` simulations per round. Zero means the
    /// default. The result is cached per simulation count.
    pub fn execute(&self, sims: usize) -> BacktestView {
        let rep = self.ctx.backtest(sims);
        BacktestView {
            rounds: rep
                .rounds
                .iter()
                .map(|r| RoundResultView {
                    round: r.round,
                    name: r.name.clone(),
                    driver_mae: r.driver_mae,
                    cons_mae: r.constructor_mae,
                    spearman_rho: r.spearman_rho,
                    grid_driver_mae: r.grid_driver_mae,
                    grid_spearman_rho: r.grid_spearman_rho,
                    grid_team_pts: r.grid_team_pts,
                    coverage: r.coverage,
                    grid_coverage: r.grid_coverage,
                    model_team_pts: r.model_team_pts,
                    naive_team_pts: r.naive_team_pts,
                    hindsight_team_pts: r.hindsight_team_pts,
                })
                .collect(),
            driver_mae: rep.driver_mae,
            cons_mae: rep.constructor_mae,
            mean_spearman: rep.mean_spearman,
            baseline_prev: rep.baseline_prev,
            baseline_season: rep.baseline_season,
            grid_driver_mae: rep.grid_driver_mae,
            grid_mean_spearman: rep.grid_mean_spearman,
            grid_team_pts: rep.grid_team_pts,
            coverage: rep.coverage,
            grid_coverage: rep.grid_coverage,
            model_team_pts: rep.model_team_pts,
            naive_team_pts: rep.naive_team_pts,
            hindsight_team_pts: rep.hindsight_team_pts,
        }
    }
}
