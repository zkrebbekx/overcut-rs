//! The walk-forward backtest of the projection model.
//!
//! For every finished round, the backtest fits the model only on the rounds
//! before it, projects the round, and scores the projection against the
//! official fantasy points. The report also scores two naive baselines, so
//! the model's edge is visible and honest.

use std::collections::HashMap;

use crate::optimizer::{best_lineups, Candidate, OptimizerOptions, TeamShape};
use crate::projection::{PaceModel, SimulationResult, SimulationSettings, WeekendConditions};
use crate::rules::ScoringRules;
use crate::season::{Round, Season};
use crate::shared::{AssetId, AssetKind, RoundNumber};

/// The first projected round. Earlier rounds have too little training
/// history.
pub const START_ROUND: u32 = 4;

/// The accuracy of one projected round.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoundResult {
    /// The round.
    pub round: u32,
    /// The race name.
    pub name: String,
    /// The mean absolute driver error, pre-qualifying.
    pub driver_mae: f64,
    /// The mean absolute constructor error.
    pub constructor_mae: f64,
    /// The Spearman rank correlation over drivers.
    pub spearman_rho: f64,
    /// The driver error with qualifying and the grid known.
    pub grid_driver_mae: f64,
    /// The rank correlation with the grid known.
    pub grid_spearman_rho: f64,
    /// The projected-optimal team's real points with the grid known.
    pub grid_team_pts: f64,
    /// The share of drivers whose actual points fell inside P10–P90.
    pub coverage: f64,
    /// The same with the grid known.
    pub grid_coverage: f64,
    /// What the projected optimal team really scored.
    pub model_team_pts: f64,
    /// What the "last week's scorers" team really scored.
    pub naive_team_pts: f64,
    /// The hindsight optimum.
    pub hindsight_team_pts: f64,
}

/// The pooled backtest result.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BacktestReport {
    /// One result per projected round.
    pub rounds: Vec<RoundResult>,
    /// The mean driver error.
    pub driver_mae: f64,
    /// The mean constructor error.
    pub constructor_mae: f64,
    /// The mean rank correlation.
    pub mean_spearman: f64,
    /// The driver error of the previous-round baseline.
    pub baseline_prev: f64,
    /// The driver error of the season-mean baseline.
    pub baseline_season: f64,
    /// The driver error with the grid known.
    pub grid_driver_mae: f64,
    /// The rank correlation with the grid known.
    pub grid_mean_spearman: f64,
    /// The mean team points with the grid known.
    pub grid_team_pts: f64,
    /// The share of drivers inside P10–P90, pre-qualifying.
    pub coverage: f64,
    /// The same with the grid known.
    pub grid_coverage: f64,
    /// The mean model team points.
    pub model_team_pts: f64,
    /// The mean naive team points.
    pub naive_team_pts: f64,
    /// The mean hindsight team points.
    pub hindsight_team_pts: f64,
}

/// Builds the grid-known conditions of a finished round: the qualifying
/// classification, the official grid, and the sprint result.
pub fn known_weekend(round: &Round) -> WeekendConditions {
    let mut cond = WeekendConditions::default();
    if let Some(order) = round.quali_order() {
        cond.quali = WeekendConditions::positions(&order);
    }
    if let Some(order) = round.grid_order() {
        cond.grid = WeekendConditions::positions(&order);
    }
    if let Some(sprint) = round.sprint_order() {
        cond = cond.with_sprint(&sprint.finish, &sprint.grid, sprint.dnf);
    }
    cond
}

/// Runs the walk-forward backtest with the given settings per round.
pub fn run(season: &Season, rules: &ScoringRules, settings: SimulationSettings) -> BacktestReport {
    let mut rep = BacktestReport::default();
    let mut pooled_prev = Vec::new();
    let mut pooled_season = Vec::new();

    for round in season
        .completed_rounds()
        .filter(|r| r.number.get() >= START_ROUND)
    {
        let model = PaceModel::fit(season, rules, round.number.previous());
        let sim = model.simulate(round.number, round.has_sprint, settings);
        let grid_sim = model.simulate_with(
            round.number,
            round.has_sprint,
            settings,
            &known_weekend(round),
        );

        let mut rr = RoundResult {
            round: round.number.get(),
            name: round.name.clone(),
            ..Default::default()
        };
        let (mut proj_d, mut act_d, mut grid_d) = (Vec::new(), Vec::new(), Vec::new());
        let (mut n_d, mut n_c) = (0.0, 0.0);
        let gameday = round.number.gameday();
        for a in season.assets() {
            let Some(actual) = a.at(gameday) else {
                continue;
            };
            let Some(proj) = sim.by_id(&a.id) else {
                continue;
            };
            let Some(gproj) = grid_sim.by_id(&a.id) else {
                continue;
            };
            let err = (proj.dist.mean - actual.points).abs();
            if a.kind == AssetKind::Driver {
                rr.driver_mae += err;
                rr.grid_driver_mae += (gproj.dist.mean - actual.points).abs();
                if actual.points >= proj.dist.p10 && actual.points <= proj.dist.p90 {
                    rr.coverage += 1.0;
                }
                if actual.points >= gproj.dist.p10 && actual.points <= gproj.dist.p90 {
                    rr.grid_coverage += 1.0;
                }
                n_d += 1.0;
                proj_d.push(proj.dist.mean);
                grid_d.push(gproj.dist.mean);
                act_d.push(actual.points);
                pooled_prev.push((a.previous_points(gameday) - actual.points).abs());
                pooled_season.push((a.season_mean_before(gameday) - actual.points).abs());
            } else {
                rr.constructor_mae += err;
                n_c += 1.0;
            }
        }
        if n_d > 0.0 {
            rr.driver_mae /= n_d;
            rr.grid_driver_mae /= n_d;
            rr.coverage /= n_d;
            rr.grid_coverage /= n_d;
        }
        if n_c > 0.0 {
            rr.constructor_mae /= n_c;
        }
        rr.spearman_rho = spearman(&proj_d, &act_d);
        rr.grid_spearman_rho = spearman(&grid_d, &act_d);
        rr.model_team_pts = team_actual_points(
            season,
            rules,
            round.number,
            &projected_candidates(season, &sim, round.number),
        );
        rr.grid_team_pts = team_actual_points(
            season,
            rules,
            round.number,
            &projected_candidates(season, &grid_sim, round.number),
        );
        rr.naive_team_pts = team_actual_points(
            season,
            rules,
            round.number,
            &candidates_priced_at(season, round.number, |a| a.previous_points(gameday)),
        );
        rr.hindsight_team_pts = team_actual_points(
            season,
            rules,
            round.number,
            &candidates_priced_at(season, round.number, |a| {
                a.at(gameday).map_or(0.0, |h| h.points)
            }),
        );
        rep.rounds.push(rr);
    }

    let n = rep.rounds.len() as f64;
    if n == 0.0 {
        return rep;
    }
    for rr in &rep.rounds {
        rep.driver_mae += rr.driver_mae / n;
        rep.constructor_mae += rr.constructor_mae / n;
        rep.mean_spearman += rr.spearman_rho / n;
        rep.grid_driver_mae += rr.grid_driver_mae / n;
        rep.grid_mean_spearman += rr.grid_spearman_rho / n;
        rep.grid_team_pts += rr.grid_team_pts / n;
        rep.coverage += rr.coverage / n;
        rep.grid_coverage += rr.grid_coverage / n;
        rep.model_team_pts += rr.model_team_pts / n;
        rep.naive_team_pts += rr.naive_team_pts / n;
        rep.hindsight_team_pts += rr.hindsight_team_pts / n;
    }
    rep.baseline_prev = crate::projection::mean(&pooled_prev);
    rep.baseline_season = crate::projection::mean(&pooled_season);
    rep
}

/// Converts a simulation into optimizer candidates priced at the round's
/// real prices.
fn projected_candidates(
    season: &Season,
    sim: &SimulationResult,
    round: RoundNumber,
) -> Vec<Candidate> {
    let gameday = round.gameday();
    season
        .assets()
        .iter()
        .filter_map(|a| {
            let h = a.at(gameday)?;
            let p = sim.by_id(&a.id)?;
            Some(Candidate {
                id: a.id.clone(),
                name: a.name.clone(),
                kind: a.kind,
                price: h.price,
                points: p.dist.mean,
            })
        })
        .collect()
}

/// Prices every asset at the round's real price with the given points.
fn candidates_priced_at(
    season: &Season,
    round: RoundNumber,
    points: impl Fn(&crate::season::Asset) -> f64,
) -> Vec<Candidate> {
    let gameday = round.gameday();
    season
        .assets()
        .iter()
        .filter_map(|a| {
            let h = a.at(gameday)?;
            Some(Candidate {
                id: a.id.clone(),
                name: a.name.clone(),
                kind: a.kind,
                price: h.price,
                points: points(a),
            })
        })
        .collect()
}

/// Optimizes a team on the given candidates, then scores that team with
/// the round's official points, captain included. The captain is chosen
/// before the race on projected points; the double lands on the projected
/// captain's actual points.
fn team_actual_points(
    season: &Season,
    rules: &ScoringRules,
    round: RoundNumber,
    candidates: &[Candidate],
) -> f64 {
    let shape = TeamShape {
        drivers: rules.team_drivers,
        constructors: rules.team_constructors,
    };
    let Some(team) = best_lineups(candidates, &OptimizerOptions::new(rules.budget, shape))
        .into_iter()
        .next()
    else {
        return 0.0;
    };
    let gameday = round.gameday();
    let actual: HashMap<&AssetId, f64> = season
        .assets()
        .iter()
        .filter_map(|a| a.at(gameday).map(|h| (&a.id, h.points)))
        .collect();
    let pts = |id: &AssetId| actual.get(id).copied().unwrap_or(0.0);
    team.assets().map(|a| pts(&a.id)).sum::<f64>() + pts(&team.captain_id)
}

/// Average ranks, with ties sharing the mean rank.
fn ranks(v: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|a, b| v[*a].total_cmp(&v[*b]));
    let mut out = vec![0.0; v.len()];
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j < idx.len() && v[idx[j]] == v[idx[i]] {
            j += 1;
        }
        let avg = (i + j - 1) as f64 / 2.0 + 1.0;
        for k in i..j {
            out[idx[k]] = avg;
        }
        i = j;
    }
    out
}

/// The Spearman rank correlation of two paired samples. Returns zero for
/// fewer than three pairs or a constant sample.
pub fn spearman(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.len() < 3 {
        return 0.0;
    }
    let (ra, rb) = (ranks(a), ranks(b));
    let n = ra.len() as f64;
    let ma = ra.iter().sum::<f64>() / n;
    let mb = rb.iter().sum::<f64>() / n;
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for (x, y) in ra.iter().zip(&rb) {
        cov += (x - ma) * (y - mb);
        va += (x - ma) * (x - ma);
        vb += (y - mb) * (y - mb);
    }
    if va == 0.0 || vb == 0.0 {
        0.0
    } else {
        cov / (va * vb).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_perfect_ordering_when_spearman_is_computed_then_it_is_one() {
        assert!((spearman(&[1.0, 2.0, 3.0, 4.0], &[10.0, 20.0, 30.0, 40.0]) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn given_a_reversed_ordering_when_spearman_is_computed_then_it_is_minus_one() {
        assert!((spearman(&[1.0, 2.0, 3.0], &[3.0, 2.0, 1.0]) + 1.0).abs() < 1e-12);
    }

    #[test]
    fn given_ties_when_ranked_then_they_share_the_mean_rank() {
        assert_eq!(ranks(&[5.0, 1.0, 5.0]), vec![2.5, 1.0, 2.5]);
    }

    #[test]
    fn given_too_few_pairs_when_spearman_is_computed_then_it_is_zero() {
        assert_eq!(spearman(&[1.0, 2.0], &[2.0, 1.0]), 0.0);
    }
}
