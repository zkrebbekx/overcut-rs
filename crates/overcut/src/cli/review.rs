//! `overcut review`: projection against official points for a finished
//! round.

use std::io::Write;

use chrono::Utc;
use clap::Args;
use overcut_application::dto::ReviewInput;
use overcut_application::usecases::{resolve_team, ReviewRound};

use super::{flush, table, GlobalOpts};
use crate::text::split_opt;

/// The `review` flags.
#[derive(Debug, Clone, Args)]
pub struct ReviewArgs {
    /// Round (default: latest completed).
    #[arg(long, default_value_t = 0, value_name = "N")]
    pub round: u32,

    /// Team held for the round: driver TLAs and constructor names.
    #[arg(long, value_name = "LIST")]
    pub team: Option<String>,

    /// Simulation count.
    #[arg(long, default_value_t = 20_000)]
    pub sims: usize,
}

/// Runs the command.
pub fn run(g: &GlobalOpts, args: &ReviewArgs) -> anyhow::Result<()> {
    let ctx = g.context()?;
    let tokens = split_opt(args.team.as_deref());
    let ids: Vec<String> = resolve_team(&ctx.season(), &tokens)?
        .iter()
        .map(ToString::to_string)
        .collect();
    let has_team = !ids.is_empty();

    let view = ReviewRound::new(&ctx).execute(
        ReviewInput {
            round: args.round,
            sims: args.sims,
            seed: 0,
            team: ids,
            captain: String::new(),
        },
        Utc::now(),
    )?;

    println!(
        "Round {} — {} — projection (grid known) vs official points\n",
        view.round, view.name
    );
    let mut w = table();
    writeln!(w, "ASSET\tPROJ\tP10\tP90\tACTUAL\tDELTA\tZ\tHELD")?;
    for a in &view.assets {
        let held = if a.held { "•" } else { "" };
        writeln!(
            w,
            "{}\t{:.1}\t{:.0}\t{:.0}\t{:.0}\t{:+.1}\t{:+.1}\t{}",
            a.name, a.projected, a.p10, a.p90, a.actual, a.delta, a.z, held
        )?;
    }
    flush(w)?;
    println!(
        "\nDriver MAE {:.1} · {:.0}% of drivers inside P10–P90 · hindsight optimum {:.0} pts",
        view.driver_mae,
        view.coverage * 100.0,
        view.hindsight_points
    );
    if has_team {
        println!(
            "Your team: projected {:.1}, actual {:.0}",
            view.team_projected, view.team_actual
        );
    }
    Ok(())
}
