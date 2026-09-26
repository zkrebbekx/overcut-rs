//! `overcut hindsight`: the best teams for a finished round.

use clap::Args;
use overcut_application::usecases::Hindsight;

use super::GlobalOpts;

/// The `hindsight` flags.
#[derive(Debug, Clone, Args)]
pub struct HindsightArgs {
    /// Round (default: latest completed).
    #[arg(long, default_value_t = 0, value_name = "N")]
    pub round: u32,

    /// Teams to show.
    #[arg(long, default_value_t = 5, value_name = "N")]
    pub top: usize,
}

/// Runs the command.
pub fn run(g: &GlobalOpts, args: &HindsightArgs) -> anyhow::Result<()> {
    let ctx = g.context()?;
    let view = Hindsight::new(&ctx).execute(args.round, args.top)?;

    println!(
        "Round {} — {} — hindsight-optimal teams (official points)\n",
        view.round, view.name
    );
    for (i, t) in view.teams.iter().enumerate() {
        let mut parts: Vec<String> = t
            .drivers
            .iter()
            .map(|d| {
                let mark = if d.id == t.captain_id { "*" } else { "" };
                format!("{}{mark} {:.0}", d.name, d.points)
            })
            .collect();
        parts.extend(
            t.constructors
                .iter()
                .map(|c| format!("[{}] {:.0}", c.name, c.points)),
        );
        println!(
            "#{}  {:.0} pts  ${:.1}M\n    {}",
            i + 1,
            t.score,
            t.cost,
            parts.join(", ")
        );
    }
    println!("\n* = captain (DRS boost)");
    Ok(())
}
