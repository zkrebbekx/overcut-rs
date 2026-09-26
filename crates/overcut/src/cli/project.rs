//! `overcut project`: project one round.

use std::io::Write;

use clap::Args;
use overcut_application::dto::ProjectInput;
use overcut_application::usecases::ProjectRound;

use super::{flush, table, ConditionArgs, GlobalOpts};

/// The `project` flags.
#[derive(Debug, Clone, Args)]
pub struct ProjectArgs {
    /// Simulation count.
    #[arg(long, default_value_t = 20_000)]
    pub sims: usize,

    /// Round to project (default: next).
    #[arg(long, default_value_t = 0, value_name = "N")]
    pub round: u32,

    /// Random seed.
    #[arg(long, default_value_t = 1)]
    pub seed: u64,

    /// The known weekend state.
    #[command(flatten)]
    pub conditions: ConditionArgs,
}

/// Runs the command.
pub fn run(g: &GlobalOpts, args: &ProjectArgs) -> anyhow::Result<()> {
    let ctx = g.context()?;
    let view = ProjectRound::new(&ctx).execute(ProjectInput {
        round: args.round,
        sims: args.sims,
        seed: args.seed,
        conditions: args.conditions.to_input(),
    })?;

    let sprint = if view.has_sprint {
        " (sprint weekend)"
    } else {
        ""
    };
    println!(
        "Round {} — {}{} — {} sims\n",
        view.round, view.name, sprint, view.sims
    );

    let mut w = table();
    writeln!(w, "ASSET\tKIND\tPRICE\txPTS\tP10\tP50\tP90\tPTS/$M\tOWN%")?;
    for a in &view.assets {
        let per_million = if a.price > 0.0 { a.mean / a.price } else { 0.0 };
        writeln!(
            w,
            "{}\t{}\t{:.1}\t{:.1}\t{:.0}\t{:.0}\t{:.0}\t{:.2}\t{:.0}",
            a.name, a.kind, a.price, a.mean, a.p10, a.p50, a.p90, per_million, a.ownership
        )?;
    }
    flush(w)
}
