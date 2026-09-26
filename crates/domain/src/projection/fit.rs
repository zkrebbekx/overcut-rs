//! Fitting the pace model to a season.

use std::collections::BTreeMap;

use crate::rules::{DriverWeekend, ScoringRules};
use crate::season::{Asset, RaceRow, Round, Season};
use crate::shared::{AssetId, RoundNumber, TeamId, Tla};

/// The form half-life in rounds. A result N rounds old carries weight
/// 0.5^(N / `HALF_LIFE`).
pub const HALF_LIFE: f64 = 4.0;

/// The fitted state of one driver.
#[derive(Debug, Clone, PartialEq)]
pub struct DriverPace {
    /// The fantasy asset.
    pub asset_id: AssetId,
    /// The driver code.
    pub tla: Tla,
    /// The display name.
    pub name: String,
    /// The team the driver drives for.
    pub team_id: TeamId,
    /// The EWMA qualifying position.
    pub quali_mu: f64,
    /// The spread of the qualifying position.
    pub quali_sd: f64,
    /// The EWMA race finish position among classified cars.
    pub race_mu: f64,
    /// The spread of the race finish position.
    pub race_sd: f64,
    /// The per-race retirement probability.
    pub dnf_prob: f64,
    /// The expected overtake points per weekend.
    pub overtake_lambda: f64,
    /// The fastest-lap probability.
    pub fastest_lap_prob: f64,
    /// The driver-of-the-day probability.
    pub dotd_prob: f64,
    /// The count of rounds with data.
    pub rounds: u32,
}

/// The fitted state of one constructor.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstructorPace {
    /// The fantasy asset.
    pub asset_id: AssetId,
    /// The display name.
    pub name: String,
    /// The team.
    pub team_id: TeamId,
    /// Its drivers' codes.
    pub tlas: Vec<Tla>,
    /// The EWMA residual: official points minus rule-derived points. It
    /// captures the pit-stop component and any systematic table difference.
    pub pit_residual: f64,
}

/// A full fitted season model.
#[derive(Debug, Clone, PartialEq)]
pub struct PaceModel {
    /// The rules the model scores with.
    pub rules: ScoringRules,
    /// The fitted drivers, in feed order.
    pub drivers: Vec<DriverPace>,
    /// The fitted constructors, in feed order.
    pub constructors: Vec<ConstructorPace>,
}

/// The EWMA weight for a result that is `age` rounds old.
fn weight(age: u32) -> f64 {
    0.5_f64.powf(f64::from(age) / HALF_LIFE)
}

/// A weighted mean and a shrunk weighted standard deviation over (value,
/// age) observations. The standard deviation shrinks toward `prior_sd`
/// with a prior strength of two observations. Returns (mean, sd, weight
/// sum).
fn ewma(obs: &[(f64, u32)], prior_sd: f64) -> (f64, f64, f64) {
    let sw: f64 = obs.iter().map(|(_, age)| weight(*age)).sum();
    if sw == 0.0 {
        return (0.0, prior_sd, 0.0);
    }
    let mu = obs.iter().map(|(v, age)| weight(*age) * v).sum::<f64>() / sw;
    let swv: f64 = obs
        .iter()
        .map(|(v, age)| weight(*age) * (v - mu) * (v - mu))
        .sum();
    const PRIOR_STRENGTH: f64 = 2.0;
    let variance = (swv + PRIOR_STRENGTH * prior_sd * prior_sd) / (sw + PRIOR_STRENGTH);
    (mu, variance.sqrt(), sw)
}

/// A smoothed event rate with a Beta-style prior.
fn shrunk_rate(events: f64, trials: f64, prior_rate: f64, prior_strength: f64) -> f64 {
    (events + prior_rate * prior_strength) / (trials + prior_strength)
}

/// The mean retirement rate per car per race, shrunk toward a historic
/// base rate of 0.10 so a short or clean sample keeps a realistic floor.
fn field_dnf_rate(completed: &[&Round]) -> f64 {
    let cars = completed.iter().map(|r| r.race.len()).sum::<usize>() as f64;
    let dnfs = completed
        .iter()
        .flat_map(|r| r.race.values())
        .filter(|row| row.dnf)
        .count() as f64;
    const BASE_RATE: f64 = 0.10;
    const BASE_STRENGTH: f64 = 40.0;
    (dnfs + BASE_RATE * BASE_STRENGTH) / (cars + BASE_STRENGTH)
}

/// Builds a [`DriverWeekend`] from the classifications of one round,
/// without overtakes or the driver-of-the-day award.
pub(crate) fn weekend_from_results(round: &Round, tla: &Tla, row: RaceRow) -> DriverWeekend {
    let mut w = DriverWeekend {
        quali_pos: round.quali.get(tla).copied().unwrap_or(0),
        grid_pos: row.grid,
        finish_pos: row.pos,
        dnf: row.dnf,
        fastest_lap: row.fastest_lap,
        ..Default::default()
    };
    if row.grid == 0 {
        // A pit-lane start counts as a slot behind the last car.
        w.grid_pos = round.race.len() as u32 + 1;
    }
    if round.has_sprint {
        if let Some(s) = round.sprint.get(tla) {
            w.has_sprint = true;
            w.sprint_grid = if s.grid == 0 {
                round.sprint.len() as u32 + 1
            } else {
                s.grid
            };
            w.sprint_pos = s.pos;
            w.sprint_dnf = s.dnf;
        }
    }
    w
}

impl PaceModel {
    /// Fits a model on every completed round up to and including `through`.
    /// `None` fits on no round at all, which yields the priors.
    pub fn fit(season: &Season, rules: &ScoringRules, through: Option<RoundNumber>) -> Self {
        let completed: Vec<&Round> = season
            .completed_rounds()
            .filter(|r| through.is_some_and(|t| r.number <= t))
            .collect();
        let latest = completed.last().map_or(0, |r| r.number.get());
        let age_of = |r: &Round| latest - r.number.get();

        let field_dnf = field_dnf_rate(&completed);
        let mut by_team: BTreeMap<TeamId, Vec<Tla>> = BTreeMap::new();
        let mut drivers = Vec::new();

        for a in season.drivers() {
            // A driver that lost the seat in a mid-season swap stays in the
            // dataset but takes no part in the simulated field.
            if !season.selectable(a) {
                continue;
            }
            let Some(tla) = a.tla.clone() else { continue };
            let mut q_obs = Vec::new();
            let mut r_obs = Vec::new();
            let (mut dnfs, mut races, mut fls) = (0.0, 0.0, 0.0);
            for r in &completed {
                let age = age_of(r);
                if let Some(&p) = r.quali.get(&tla) {
                    if p > 0 {
                        q_obs.push((f64::from(p), age));
                    }
                }
                if let Some(row) = r.race.get(&tla) {
                    races += 1.0;
                    if row.dnf {
                        dnfs += 1.0;
                    } else {
                        r_obs.push((f64::from(row.pos), age));
                    }
                    if row.fastest_lap {
                        fls += 1.0;
                    }
                }
            }
            let (mut quali_mu, quali_sd, _) = ewma(&q_obs, 2.5);
            let (mut race_mu, race_sd, _) = ewma(&r_obs, 3.5);
            if q_obs.is_empty() {
                quali_mu = 11.0;
            }
            if r_obs.is_empty() {
                race_mu = quali_mu;
            }

            // Overtake points per weekend are exact: the official race and
            // sprint points minus what the rules derive from the
            // classifications alone. The feed's cumulative overtake stat
            // lags by a round, so the residual is the reliable source. The
            // driver-of-the-day award comes from the feed stat.
            let dotd: BTreeMap<u32, f64> = a
                .component_deltas(|s| s.dotd_pts)
                .into_iter()
                .map(|(g, v)| (g.get(), v))
                .collect();
            let mut o_obs = Vec::new();
            let mut dotd_count = 0.0;
            for r in &completed {
                let n = r.number.get();
                if dotd.get(&n).is_some_and(|v| *v > 0.0) {
                    dotd_count += 1.0;
                }
                let Some(official) = a.at(r.number.gameday()) else {
                    continue;
                };
                let Some(row) = r.race.get(&tla) else {
                    continue;
                };
                let mut w = weekend_from_results(r, &tla, *row);
                w.dotd = dotd.get(&n).is_some_and(|v| *v > 0.0);
                let quali_only = DriverWeekend {
                    quali_pos: w.quali_pos,
                    ..Default::default()
                };
                let base = rules.driver_points(w) - rules.driver_points(quali_only);
                let resid = official.race_pts + official.sprint_pts - f64::from(base);
                o_obs.push((resid.max(0.0), age_of(r)));
            }
            let (lambda, _, _) = ewma(&o_obs, 0.0);

            by_team
                .entry(a.team_id.clone())
                .or_default()
                .push(tla.clone());
            drivers.push(DriverPace {
                asset_id: a.id.clone(),
                tla,
                name: a.name.clone(),
                team_id: a.team_id.clone(),
                quali_mu,
                quali_sd,
                race_mu,
                race_sd,
                dnf_prob: shrunk_rate(dnfs, races, field_dnf, 8.0),
                overtake_lambda: lambda.max(0.0),
                fastest_lap_prob: shrunk_rate(fls, races, 1.0 / 22.0, 4.0),
                dotd_prob: shrunk_rate(dotd_count, races, 1.0 / 22.0, 4.0),
                rounds: races as u32,
            });
        }

        let constructors = season
            .constructors()
            .filter(|a| season.selectable(a))
            .map(|a| ConstructorPace {
                asset_id: a.id.clone(),
                name: a.name.clone(),
                team_id: a.team_id.clone(),
                tlas: by_team.get(&a.team_id).cloned().unwrap_or_default(),
                pit_residual: fit_pit_residual(season, rules, a, &completed, latest),
            })
            .collect();

        Self {
            rules: rules.clone(),
            drivers,
            constructors,
        }
    }

    /// Fits a model on every completed round.
    pub fn fit_all(season: &Season, rules: &ScoringRules) -> Self {
        Self::fit(
            season,
            rules,
            season.latest_completed_round().map(|r| r.number),
        )
    }

    /// Returns the fitted driver with the given code.
    pub fn driver_by_tla(&self, tla: &Tla) -> Option<&DriverPace> {
        self.drivers.iter().find(|d| &d.tla == tla)
    }

    /// Returns the drivers ordered by race pace, best first.
    pub fn sorted_drivers(&self) -> Vec<&DriverPace> {
        let mut out: Vec<&DriverPace> = self.drivers.iter().collect();
        out.sort_by(|a, b| a.race_mu.total_cmp(&b.race_mu));
        out
    }
}

/// The EWMA residual between a constructor's official gameday points and
/// the points that the rules derive from its drivers' official points plus
/// the qualifying bonus.
fn fit_pit_residual(
    season: &Season,
    rules: &ScoringRules,
    cons: &Asset,
    completed: &[&Round],
    latest: u32,
) -> f64 {
    struct DriverSeries {
        tla: Tla,
        points: BTreeMap<u32, f64>,
        dotd: BTreeMap<u32, f64>,
    }
    let series: Vec<DriverSeries> = season
        .drivers()
        .filter(|d| d.team_id == cons.team_id)
        .filter_map(|d| {
            let tla = d.tla.clone()?;
            Some(DriverSeries {
                tla,
                points: d
                    .history
                    .iter()
                    .map(|h| (h.gameday.get(), h.points))
                    .collect(),
                dotd: d
                    .component_deltas(|s| s.dotd_pts)
                    .into_iter()
                    .map(|(g, v)| (g.get(), v))
                    .collect(),
            })
        })
        .collect();
    if series.is_empty() {
        return 0.0;
    }
    let mut obs = Vec::new();
    for r in completed {
        let n = r.number.get();
        let Some(official) = cons.at(r.number.gameday()) else {
            continue;
        };
        let mut derived = 0.0;
        let (mut in_q2, mut in_q3) = (0, 0);
        let mut have_all = true;
        for d in &series {
            let Some(p) = d.points.get(&n) else {
                have_all = false;
                break;
            };
            derived += p - d.dotd.get(&n).copied().unwrap_or(0.0);
            if let Some(&qp) = r.quali.get(&d.tla) {
                if qp >= 1 && qp <= rules.q2_cutoff {
                    in_q2 += 1;
                }
                if qp >= 1 && qp <= rules.q3_cutoff {
                    in_q3 += 1;
                }
            }
        }
        if !have_all {
            continue;
        }
        derived += f64::from(rules.constructor_quali_bonus_for(in_q2, in_q3));
        obs.push((official.points - derived, latest - n));
    }
    ewma(&obs, 0.0).0
}
