//! `overcut optimize`: find the best team for a round.

use clap::Args;
use overcut_application::dto::{OptimizeInput, TeamView};
use overcut_application::usecases::{resolve_team, OptimizeTeam};

use super::{ConditionArgs, GlobalOpts};
use crate::text::split_opt;

/// The `optimize` flags.
#[derive(Debug, Clone, Args)]
pub struct OptimizeArgs {
    /// Simulation count.
    #[arg(long, default_value_t = 20_000)]
    pub sims: usize,

    /// Round to optimize for (default: next).
    #[arg(long, default_value_t = 0, value_name = "N")]
    pub round: u32,

    /// Random seed.
    #[arg(long, default_value_t = 1)]
    pub seed: u64,

    /// Current team: driver TLAs and constructor names, comma-separated.
    #[arg(long, value_name = "LIST")]
    pub team: Option<String>,

    /// Free transfers available.
    #[arg(long, default_value_t = 2, value_name = "N")]
    pub free: u32,

    /// Chip to play: wildcard | limitless | 3x | nonegative.
    #[arg(long, value_name = "NAME")]
    pub chip: Option<String>,

    /// Budget in millions (default from rules).
    #[arg(long, default_value_t = 0.0, value_name = "X")]
    pub budget: f64,

    /// Projection quantile to optimize: mean | p10 | p90.
    #[arg(long, value_name = "MODE", default_value = "mean")]
    pub risk: String,

    /// Teams to show.
    #[arg(long, default_value_t = 5, value_name = "N")]
    pub top: usize,

    /// The known weekend state.
    #[command(flatten)]
    pub conditions: ConditionArgs,
}

/// Formats the members of a team: `*` marks the captain, `+` the regular
/// Boost under the x3 chip, and constructors sit in brackets.
fn members(t: &TeamView) -> String {
    let mut parts: Vec<String> = t
        .drivers
        .iter()
        .map(|d| {
            let mark = if d.id == t.captain_id {
                "*"
            } else if d.id == t.boost_id {
                "+"
            } else {
                ""
            };
            format!("{}{mark}", d.name)
        })
        .collect();
    parts.extend(t.constructors.iter().map(|c| format!("[{}]", c.name)));
    parts.join(", ")
}

/// Runs the command.
pub fn run(g: &GlobalOpts, args: &OptimizeArgs) -> anyhow::Result<()> {
    let ctx = g.context()?;
    let tokens = split_opt(args.team.as_deref());
    let current: Vec<String> = resolve_team(&ctx.season(), &tokens)?
        .iter()
        .map(ToString::to_string)
        .collect();
    let has_team = !current.is_empty();

    let view = OptimizeTeam::new(&ctx).execute(OptimizeInput {
        round: args.round,
        sims: args.sims,
        seed: args.seed,
        team: current,
        free_transfers: args.free,
        budget: args.budget,
        chip: args.chip.clone().unwrap_or_default(),
        risk: args.risk.clone(),
        top: args.top,
        conditions: args.conditions.to_input(),
    })?;
    if view.teams.is_empty() {
        anyhow::bail!("no legal team fits the budget");
    }

    println!(
        "Round {} — {} — best teams by {} projection\n",
        view.round, view.name, view.risk
    );
    for (i, t) in view.teams.iter().enumerate() {
        print!("#{}  {:.1} pts  ${:.1}M", i + 1, t.score, t.cost);
        if has_team {
            print!("  ({} transfers, penalty {:.0})", t.transfers, t.penalty);
        }
        println!("\n    {}", members(t));
    }
    if view.chip == "3x" {
        println!("\n* = x3 chip, + = regular 2x Boost");
    } else {
        println!("\n* = 2x Boost");
    }

    if !view.chips.is_empty() {
        println!("\nChips this round:");
        for c in &view.chips {
            if c.available {
                println!("  {}: {:+.1} ({})", c.label, c.gain, c.note);
            } else {
                println!("  {}: n/a ({})", c.label, c.note);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use overcut_application::dto::TeamAsset;

    use super::*;

    fn asset(id: &str, name: &str, kind: &str) -> TeamAsset {
        TeamAsset {
            id: id.into(),
            name: name.into(),
            kind: kind.into(),
            price: 10.0,
            points: 1.0,
        }
    }

    #[test]
    fn given_a_team_with_a_captain_and_a_boost_when_formatted_then_the_marks_and_brackets_appear() {
        let t = TeamView {
            drivers: vec![asset("1", "Max", "driver"), asset("2", "Lando", "driver")],
            constructors: vec![asset("3", "McLaren", "constructor")],
            captain_id: "1".into(),
            boost_id: "2".into(),
            cost: 0.0,
            raw_points: 0.0,
            captain_points: 0.0,
            transfers: 0,
            penalty: 0.0,
            score: 0.0,
            bought: vec![],
            sold: vec![],
            p10: 0.0,
            p50: 0.0,
            p90: 0.0,
        };
        assert_eq!(members(&t), "Max*, Lando+, [McLaren]");
    }
}
