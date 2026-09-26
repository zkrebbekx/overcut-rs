//! Find the best team for a round.

use std::collections::{HashMap, HashSet};

use overcut_domain::optimizer::{best_lineups, Candidate, Lineup, OptimizerOptions, TeamShape};
use overcut_domain::projection::{mean, Distribution, Projection, SimulationResult};
use overcut_domain::season::Round;
use overcut_domain::shared::{AssetId, AssetKind};

use crate::dto::{
    Chip, ChipValue, ConditionsInput, OptimizeInput, OptimizeView, RiskMode, TeamAsset, TeamView,
};
use crate::AppError;

use super::weekend::{projection_view, round_number, to_domain, with_known_weekend};
use super::AnalysisContext;

/// Enumerates every legal team for a round and values every chip against
/// the recommendation.
pub struct OptimizeTeam<'a> {
    ctx: &'a AnalysisContext,
}

/// Converts a lineup to its view, with the transfers against `current`.
pub(crate) fn team_view(t: &Lineup, current: &HashSet<AssetId>) -> TeamView {
    let asset = |c: &Candidate| TeamAsset {
        id: c.id.to_string(),
        name: c.name.clone(),
        kind: c.kind.as_str().to_string(),
        price: c.price,
        points: c.points,
    };
    let in_team = t.ids();
    let mut bought: Vec<String> = in_team
        .difference(current)
        .map(ToString::to_string)
        .collect();
    let mut sold: Vec<String> = current
        .difference(&in_team)
        .map(ToString::to_string)
        .collect();
    bought.sort();
    sold.sort();
    TeamView {
        drivers: t.drivers.iter().map(asset).collect(),
        constructors: t.constructors.iter().map(asset).collect(),
        captain_id: t.captain_id.to_string(),
        boost_id: t
            .boost_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
        cost: t.cost,
        raw_points: t.raw_points,
        captain_points: t.captain_points,
        transfers: t.transfers,
        penalty: t.penalty,
        score: t.score,
        bought,
        sold,
        p10: 0.0,
        p50: 0.0,
        p90: 0.0,
    }
}

/// Sums the joint samples of a team, with the boosts and the transfer
/// penalty, under normal or No Negative scoring.
fn team_samples(sim: &SimulationResult, t: &Lineup, no_negative: bool) -> Vec<f64> {
    let src = if no_negative {
        &sim.samples_no_negative
    } else {
        &sim.samples
    };
    let mut out = vec![t.penalty; sim.sims];
    for a in t.assets() {
        let Some(s) = src.get(&a.id) else { continue };
        if s.len() != sim.sims {
            continue;
        }
        let mult = if a.kind == AssetKind::Driver {
            t.multiplier(&a.id)
        } else {
            1.0
        };
        for (o, v) in out.iter_mut().zip(s) {
            *o += mult * v;
        }
    }
    out
}

/// The expected gain of assigning the Boost after the race to the team's
/// best driver, over the pre-chosen captain.
fn autopilot_gain(sim: &SimulationResult, t: &Lineup) -> f64 {
    if sim.sims == 0 {
        return 0.0;
    }
    let series: Vec<(&Vec<f64>, bool)> = t
        .drivers
        .iter()
        .filter_map(|d| {
            sim.samples
                .get(&d.id)
                .filter(|s| s.len() == sim.sims)
                .map(|s| (s, d.id == t.captain_id))
        })
        .collect();
    let mut gain = 0.0;
    for i in 0..sim.sims {
        let mut best = f64::NEG_INFINITY;
        let mut chosen = 0.0;
        for (s, is_captain) in &series {
            best = best.max(s[i]);
            if *is_captain {
                chosen = s[i];
            }
        }
        if best > f64::NEG_INFINITY {
            gain += best - chosen;
        }
    }
    gain / sim.sims as f64
}

impl<'a> OptimizeTeam<'a> {
    /// Binds the use case to a context.
    pub fn new(ctx: &'a AnalysisContext) -> Self {
        Self { ctx }
    }

    /// Runs the optimizer.
    pub fn execute(&self, input: OptimizeInput) -> Result<OptimizeView, AppError> {
        let season = self.ctx.season();
        let rules = self.ctx.rules();
        let target = season.resolve_round(round_number(input.round))?;
        let chip = Chip::parse(&input.chip)?;
        let risk = RiskMode::parse(&input.risk)?;
        let top = if input.top == 0 { 5 } else { input.top };
        let current: HashSet<AssetId> = input
            .team
            .iter()
            .filter_map(|id| AssetId::new(id.clone()).ok())
            .collect();

        let (cond, known) = with_known_weekend(target, input.conditions);
        let domain_cond = to_domain(&cond)?;
        let sim = self.ctx.simulate(
            target,
            self.ctx.settings(input.sims, input.seed),
            &cond,
            &domain_cond,
        );

        let pick = |p: &Projection| match risk {
            RiskMode::Mean => p.dist.mean,
            RiskMode::P10 => p.dist.p10,
            RiskMode::P90 => p.dist.p90,
        };
        // The optimizer's candidate list from the projection, under normal
        // or No Negative scoring.
        let candidates_for = |no_negative: bool| -> (Vec<Candidate>, HashMap<AssetId, f64>) {
            let mut candidates = Vec::new();
            let mut points = HashMap::new();
            for a in season.selectable_assets() {
                let (Some(h), Some(p)) = (a.latest(), sim.by_id(&a.id)) else {
                    continue;
                };
                let v = if no_negative {
                    p.mean_no_negative
                } else {
                    pick(p)
                };
                points.insert(a.id.clone(), v);
                candidates.push(Candidate {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    kind: a.kind,
                    price: h.price,
                    points: v,
                });
            }
            (candidates, points)
        };

        let budget = if input.budget > 0.0 {
            input.budget
        } else {
            rules.budget
        };
        let shape = TeamShape {
            drivers: rules.team_drivers,
            constructors: rules.team_constructors,
        };
        let mut base_opt = OptimizerOptions::new(budget, shape)
            .top(top)
            .with_current_team(
                current.iter().cloned(),
                input.free_transfers,
                rules.transfer_penalty,
            );
        base_opt.boost_multiplier = rules.boost_multiplier;
        base_opt.extra_boost_multiplier = rules.extra_boost_multiplier;
        let with_chip = |chip: Option<Chip>| {
            let mut o = base_opt.clone();
            match chip {
                Some(Chip::Wildcard) => o.wildcard = true,
                Some(Chip::Limitless) => o.limitless = true,
                Some(Chip::TripleBoost) => o.extra_boost = true,
                Some(Chip::NoNegative) | None => {}
            }
            o
        };

        let no_negative = chip == Some(Chip::NoNegative);
        let (candidates, points) = candidates_for(no_negative);
        let teams = best_lineups(&candidates, &with_chip(chip));

        let mut view = OptimizeView {
            round: target.number.get(),
            name: target.name.clone(),
            risk: risk.as_str().to_string(),
            chip: chip.map(|c| c.as_str().to_string()).unwrap_or_default(),
            budget,
            teams: Vec::new(),
            current_score: 0.0,
            projection: projection_view(&season, target, &sim, cond.clone(), known),
            chips: Vec::new(),
        };
        for t in &teams {
            let mut tv = team_view(t, &current);
            let d = Distribution::of(&team_samples(&sim, t, no_negative));
            tv.p10 = d.p10;
            tv.p50 = d.p50;
            tv.p90 = d.p90;
            view.teams.push(tv);
        }

        // Value every chip against the best team without a chip.
        let (plain_candidates, _) = candidates_for(false);
        let base = if chip.is_none() {
            teams.first().cloned()
        } else {
            best_lineups(&plain_candidates, &with_chip(None))
                .into_iter()
                .next()
        };
        if let Some(base) = base {
            view.chips = self.chip_values(
                target,
                &sim,
                &cond,
                &base,
                &plain_candidates,
                &with_chip,
                &base_opt,
            );
        }

        // Score the current team as-is, with the Boost on its best driver.
        if !current.is_empty() {
            let mut best = f64::NEG_INFINITY;
            for id in &current {
                let v = points.get(id).copied().unwrap_or(0.0);
                view.current_score += v;
                if season
                    .asset(id)
                    .is_some_and(|a| a.kind == AssetKind::Driver)
                    && v > best
                {
                    best = v;
                }
            }
            if best > f64::NEG_INFINITY {
                view.current_score += best;
            }
        }
        Ok(view)
    }

    /// Computes the expected gain of each chip against the base team.
    #[allow(clippy::too_many_arguments)]
    fn chip_values(
        &self,
        target: &Round,
        sim: &SimulationResult,
        cond: &ConditionsInput,
        base: &Lineup,
        plain: &[Candidate],
        with_chip: &dyn Fn(Option<Chip>) -> OptimizerOptions,
        base_opt: &OptimizerOptions,
    ) -> Vec<ChipValue> {
        let season = self.ctx.season();
        let rules = self.ctx.rules();
        let base_score = base.score;
        let value = |chip: &str, label: &str, gain: f64, available: bool, note: &str| ChipValue {
            chip: chip.into(),
            label: label.into(),
            gain,
            available,
            note: note.into(),
            out_id: String::new(),
            in_id: String::new(),
        };
        let mut out = Vec::new();

        // No Negative: the same team scored with every negative category
        // floored at zero.
        out.push(value(
            "nonegative",
            "No Negative",
            mean(&team_samples(sim, base, true)) - mean(&team_samples(sim, base, false)),
            true,
            "Floors every negative scoring category at zero for each asset on the team.",
        ));
        // x3 Boost: the best team with a tripled driver and the Boost on
        // another.
        if let Some(bt) = best_lineups(plain, &with_chip(Some(Chip::TripleBoost))).first() {
            out.push(value(
                "3x",
                "x3 Boost",
                bt.score - base_score,
                true,
                "Triples one driver; the regular Boost moves to another.",
            ));
        }
        // Autopilot: the Boost lands on the best actual scorer.
        out.push(value(
            "autopilot",
            "Autopilot",
            autopilot_gain(sim, base),
            true,
            "The Boost moves to your top scorer after the race.",
        ));
        // Wildcard and Limitless: what unlimited transfers or no cost cap add.
        let has_team = !base_opt.current_team.is_empty();
        if let Some(bt) = best_lineups(plain, &with_chip(Some(Chip::Wildcard))).first() {
            out.push(if has_team {
                value(
                    "wildcard",
                    "Wildcard",
                    bt.score - base_score,
                    true,
                    "Unlimited transfers within the cost cap.",
                )
            } else {
                value(
                    "wildcard",
                    "Wildcard",
                    0.0,
                    false,
                    "Enter your current team to value unlimited transfers.",
                )
            });
        }
        if let Some(bt) = best_lineups(plain, &with_chip(Some(Chip::Limitless))).first() {
            out.push(value(
                "limitless",
                "Limitless",
                bt.score - base_score,
                true,
                "No cost cap and unlimited transfers for one round; the team restores after.",
            ));
        }
        // Final Fix: one driver swap after qualifying. The incoming driver
        // scores the race only; the outgoing driver keeps the qualifying
        // points. Only valued once the qualifying result is known.
        let mut ff = value(
            "finalfix",
            "Final Fix",
            0.0,
            false,
            "Available after qualifying: swap one driver before the race.",
        );
        if !cond.quali.is_empty() && !target.has_results {
            let quali_pts = |id: &AssetId| -> f64 {
                let Some(a) = season.asset(id) else {
                    return 0.0;
                };
                match cond.quali.iter().position(|tla| a.has_tla(tla)) {
                    Some(i) => rules.quali_points.get(i).copied().map_or(0.0, f64::from),
                    None => f64::from(rules.quali_no_time),
                }
            };
            let race_leg =
                |id: &AssetId| sim.by_id(id).map_or(0.0, |p| p.dist.mean) - quali_pts(id);
            let held: HashSet<&AssetId> = base.drivers.iter().map(|d| &d.id).collect();
            let spare = base_opt.budget - base.cost;
            let mut best_gain = 0.0;
            for out_d in &base.drivers {
                let mult = if out_d.id == base.captain_id {
                    2.0
                } else {
                    1.0
                };
                for in_a in plain
                    .iter()
                    .filter(|c| c.kind == AssetKind::Driver && !held.contains(&c.id))
                {
                    if in_a.price > out_d.price + spare + 1e-9 {
                        continue;
                    }
                    let gain = (race_leg(&in_a.id) - race_leg(&out_d.id)) * mult;
                    if gain > best_gain {
                        best_gain = gain;
                        ff.out_id = out_d.id.to_string();
                        ff.in_id = in_a.id.to_string();
                    }
                }
            }
            ff.available = true;
            ff.gain = best_gain;
            ff.note = if ff.in_id.is_empty() {
                "No swap improves the race-only projection. Keep the chip.".into()
            } else {
                "The incoming driver scores the race only; the outgoing driver keeps qualifying points.".into()
            };
        }
        out.push(ff);
        out.sort_by(|a, b| b.gain.total_cmp(&a.gain));
        out
    }
}
