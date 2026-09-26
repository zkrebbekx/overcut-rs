//! `overcut backtest`: measure the projection model on past rounds.

use std::io::Write;

use clap::Args;
use overcut_application::usecases::RunBacktest;
use overcut_domain::projection::DEFAULT_GRID_INFLUENCE;

use super::{flush, table, GlobalOpts};

/// The `backtest` flags.
///
/// The backtest always uses seed 1, so there is no `--seed` flag.
#[derive(Debug, Clone, Args)]
pub struct BacktestArgs {
    /// Simulation count per round.
    #[arg(long, default_value_t = 5_000)]
    pub sims: usize,

    /// Weight of the grid slot in the race finish (calibration).
    #[arg(long, default_value_t = DEFAULT_GRID_INFLUENCE)]
    pub grid_influence: f64,
}

/// The share of the naive-to-hindsight gap the model captures, in
/// percent. A zero gap gives zero.
pub fn gap_captured(model: f64, naive: f64, hindsight: f64) -> f64 {
    let gap = hindsight - naive;
    if gap.abs() < f64::EPSILON {
        0.0
    } else {
        100.0 * (model - naive) / gap
    }
}

/// Runs the command.
pub fn run(g: &GlobalOpts, args: &BacktestArgs) -> anyhow::Result<()> {
    let ctx = g.context()?.with_grid_influence(args.grid_influence);
    let rep = RunBacktest::new(&ctx).execute(args.sims);
    if rep.rounds.is_empty() {
        anyhow::bail!("not enough completed rounds to backtest");
    }

    println!(
        "Walk-forward backtest over {} rounds (fit on rounds before each)\n",
        rep.rounds.len()
    );
    let mut w = table();
    writeln!(
        w,
        "ROUND\tRACE\tDRV MAE\tGRID MAE\tCON MAE\tRANK ρ\tGRID ρ\tMODEL TEAM\tGRID TEAM\tNAIVE TEAM\tHINDSIGHT"
    )?;
    for r in &rep.rounds {
        writeln!(
            w,
            "{}\t{}\t{:.1}\t{:.1}\t{:.1}\t{:.2}\t{:.2}\t{:.0}\t{:.0}\t{:.0}\t{:.0}",
            r.round,
            r.name,
            r.driver_mae,
            r.grid_driver_mae,
            r.cons_mae,
            r.spearman_rho,
            r.grid_spearman_rho,
            r.model_team_pts,
            r.grid_team_pts,
            r.naive_team_pts,
            r.hindsight_team_pts
        )?;
    }
    flush(w)?;

    println!(
        "\nDriver points MAE:   model {:.1} | with grid known {:.1} | last-round baseline {:.1} | season-mean baseline {:.1}",
        rep.driver_mae, rep.grid_driver_mae, rep.baseline_prev, rep.baseline_season
    );
    println!(
        "Mean driver rank ρ:  {:.2} | with grid known {:.2}",
        rep.mean_spearman, rep.grid_mean_spearman
    );
    println!(
        "P10–P90 coverage:    {:.0}% | with grid known {:.0}% (a calibrated range covers 80%)",
        rep.coverage * 100.0,
        rep.grid_coverage * 100.0
    );
    println!(
        "Mean team points:    model {:.0} | with grid known {:.0} | naive {:.0} | hindsight optimum {:.0}",
        rep.model_team_pts, rep.grid_team_pts, rep.naive_team_pts, rep.hindsight_team_pts
    );
    println!(
        "Model captures {:.0}% of the naive→hindsight gap.",
        gap_captured(
            rep.model_team_pts,
            rep.naive_team_pts,
            rep.hindsight_team_pts
        )
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_model_halfway_between_naive_and_hindsight_when_measured_then_it_captures_fifty_percent(
    ) {
        assert!((gap_captured(150.0, 100.0, 200.0) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn given_no_gap_when_measured_then_the_share_is_zero() {
        assert_eq!(gap_captured(120.0, 100.0, 100.0), 0.0);
    }
}
