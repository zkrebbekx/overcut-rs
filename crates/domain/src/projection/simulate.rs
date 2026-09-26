//! The Monte Carlo simulation of one round.

use std::collections::HashMap;

use rand::{Rng, SeedableRng};
use rand_distr::StandardNormal;
use rand_pcg::Pcg64Dxsm;

use crate::rules::DriverWeekend;
use crate::shared::{AssetId, AssetKind, RoundNumber};

use super::stats::{mean, Distribution};
use super::{DriverPace, PaceModel, WeekendConditions};

/// The default weight of the grid slot in the race finish score. The
/// remainder of the weight goes to season race pace. The value is
/// calibrated on the walk-forward backtest with the grid known.
pub const DEFAULT_GRID_INFLUENCE: f64 = 0.35;

/// The knobs of one simulation run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimulationSettings {
    /// The count of simulated weekends.
    pub sims: usize,
    /// The random seed. The same seed and round give the same result.
    pub seed: u64,
    /// The weight of the grid slot in the race finish score.
    pub grid_influence: f64,
}

impl SimulationSettings {
    /// Builds settings with the default grid influence.
    pub fn new(sims: usize, seed: u64) -> Self {
        Self {
            sims,
            seed,
            grid_influence: DEFAULT_GRID_INFLUENCE,
        }
    }
}

impl Default for SimulationSettings {
    fn default() -> Self {
        Self::new(20_000, 1)
    }
}

/// The simulated fantasy-point distribution of one asset.
#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    /// The asset.
    pub asset_id: AssetId,
    /// The display name.
    pub name: String,
    /// Driver or constructor.
    pub kind: AssetKind,
    /// The distribution under normal scoring.
    pub dist: Distribution,
    /// The mean under the No Negative chip, which floors every negative
    /// scoring category at zero.
    pub mean_no_negative: f64,
}

/// The projections of one simulated round.
#[derive(Debug, Clone, PartialEq)]
pub struct SimulationResult {
    /// The simulated round.
    pub round: RoundNumber,
    /// The round had a sprint leg.
    pub has_sprint: bool,
    /// The count of simulated weekends.
    pub sims: usize,
    /// One projection per asset: drivers first, then constructors.
    pub projections: Vec<Projection>,
    index: HashMap<AssetId, usize>,
    /// Every simulated score per asset, in simulation order, so a caller
    /// can combine assets with their real joint distribution.
    pub samples: HashMap<AssetId, Vec<f64>>,
    /// The same under the No Negative chip.
    pub samples_no_negative: HashMap<AssetId, Vec<f64>>,
}

impl SimulationResult {
    /// Returns the projection for one asset.
    pub fn by_id(&self, id: &AssetId) -> Option<&Projection> {
        self.index.get(id).map(|i| &self.projections[*i])
    }
}

/// Ranks scores ascending into 1-based positions. Ties break on index.
fn rank(scores: &[f64]) -> Vec<u32> {
    let mut order: Vec<usize> = (0..scores.len()).collect();
    order.sort_by(|a, b| scores[*a].total_cmp(&scores[*b]).then(a.cmp(b)));
    let mut pos = vec![0; scores.len()];
    for (p, i) in order.into_iter().enumerate() {
        pos[i] = p as u32 + 1;
    }
    pos
}

/// Samples one driver index in proportion to a weight, skipping excluded
/// drivers. Returns `None` when every weight is zero.
fn pick_weighted(
    rng: &mut impl Rng,
    drivers: &[DriverPace],
    excluded: Option<&[bool]>,
    w: impl Fn(&DriverPace) -> f64,
) -> Option<usize> {
    let allowed = |i: usize| excluded.is_none_or(|e| !e[i]);
    let total: f64 = drivers
        .iter()
        .enumerate()
        .filter(|(i, _)| allowed(*i))
        .map(|(_, d)| w(d))
        .sum();
    if total <= 0.0 {
        return None;
    }
    let mut r = rng.random::<f64>() * total;
    for (i, d) in drivers.iter().enumerate() {
        if !allowed(i) {
            continue;
        }
        r -= w(d);
        if r <= 0.0 {
            return Some(i);
        }
    }
    None
}

/// Samples a Poisson count with mean `lambda` by inversion.
fn poisson(rng: &mut impl Rng, lambda: f64) -> u32 {
    if lambda <= 0.0 {
        return 0;
    }
    let l = (-lambda).exp();
    let mut k = 0;
    let mut p = 1.0;
    loop {
        p *= rng.random::<f64>();
        if p <= l || k > 50 {
            return k;
        }
        k += 1;
    }
}

impl PaceModel {
    /// Runs a Monte Carlo simulation of one round with nothing known about
    /// the weekend.
    pub fn simulate(
        &self,
        round: RoundNumber,
        has_sprint: bool,
        settings: SimulationSettings,
    ) -> SimulationResult {
        self.simulate_with(round, has_sprint, settings, &WeekendConditions::default())
    }

    /// Runs a Monte Carlo simulation of one round with the known weekend
    /// state. The same seed gives the same result.
    pub fn simulate_with(
        &self,
        round: RoundNumber,
        has_sprint: bool,
        settings: SimulationSettings,
        cond: &WeekendConditions,
    ) -> SimulationResult {
        let mut rng = Pcg64Dxsm::seed_from_u64(settings.seed ^ (u64::from(round.get()) << 32));
        let n = self.drivers.len();
        let sims = settings.sims;

        // Apply the practice prior to the pace estimates.
        let mut drivers = self.drivers.clone();
        for d in &mut drivers {
            if let Some(&p) = cond.practice.get(&d.tla) {
                if p > 0 {
                    d.quali_mu = 0.5 * d.quali_mu + 0.5 * f64::from(p);
                    d.race_mu = 0.5 * d.race_mu + 0.5 * f64::from(p);
                }
            }
        }

        // Resolves the starting grid from the known conditions given a
        // qualifying order, or `None` when nothing is known.
        let fixed_grid = |quali_pos: &[u32]| -> Option<Vec<u32>> {
            if cond.grid.is_empty() && cond.back_of_grid.is_empty() {
                return None;
            }
            if !cond.grid.is_empty() {
                return Some(
                    drivers
                        .iter()
                        .enumerate()
                        .map(|(i, d)| match cond.grid.get(&d.tla) {
                            Some(&g) if g > 0 => g,
                            _ => quali_pos[i],
                        })
                        .collect(),
                );
            }
            // Move the penalised drivers behind the rest, in qualifying order.
            let keys: Vec<f64> = drivers
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    f64::from(quali_pos[i])
                        + if cond.back_of_grid.contains(&d.tla) {
                            100.0
                        } else {
                            0.0
                        }
                })
                .collect();
            Some(rank(&keys))
        };

        let mut samples: HashMap<AssetId, Vec<f64>> = HashMap::new();
        let mut samples_nn: HashMap<AssetId, Vec<f64>> = HashMap::new();
        for id in drivers
            .iter()
            .map(|d| &d.asset_id)
            .chain(self.constructors.iter().map(|c| &c.asset_id))
        {
            samples.insert(id.clone(), Vec::with_capacity(sims));
            samples_nn.insert(id.clone(), Vec::with_capacity(sims));
        }

        let mut scores = vec![0.0; n];
        let mut weekends = vec![DriverWeekend::default(); n];
        let normal = |rng: &mut Pcg64Dxsm| -> f64 { rng.sample(StandardNormal) };

        for _ in 0..sims {
            // Qualifying: use the known classification, or sample a pace
            // score per driver and rank.
            let quali_pos: Vec<u32> = if cond.quali.is_empty() {
                for (i, d) in drivers.iter().enumerate() {
                    scores[i] = d.quali_mu + normal(&mut rng) * d.quali_sd;
                }
                rank(&scores)
            } else {
                drivers
                    .iter()
                    .map(|d| cond.quali.get(&d.tla).copied().unwrap_or(0))
                    .collect()
            };
            let grid_pos = fixed_grid(&quali_pos).unwrap_or_else(|| quali_pos.clone());

            // Race: sample retirements, then rank the classified cars by a
            // race-pace score that blends season pace with the grid slot. A
            // retired car takes no classified position.
            let mut dnf = vec![false; n];
            for (i, d) in drivers.iter().enumerate() {
                dnf[i] = rng.random::<f64>() < d.dnf_prob;
                let pace = (1.0 - settings.grid_influence) * d.race_mu
                    + settings.grid_influence * f64::from(grid_pos[i]);
                scores[i] = pace + normal(&mut rng) * d.race_sd;
                if dnf[i] {
                    scores[i] += 1000.0; // rank retired cars last
                }
            }
            let race_pos = rank(&scores);

            // Fastest lap and driver of the day: one winner each, sampled in
            // proportion to the fitted probability. A retired car cannot
            // take the fastest lap in the model.
            let fl_winner = pick_weighted(&mut rng, &drivers, Some(&dnf), |d| d.fastest_lap_prob);
            let dotd_winner = pick_weighted(&mut rng, &drivers, None, |d| d.dotd_prob);

            let sprint = if has_sprint && cond.has_sprint_result() {
                // The sprint has run: use its grid and classification.
                let sq: Vec<u32> = drivers
                    .iter()
                    .map(|d| match cond.sprint_grid.get(&d.tla) {
                        Some(&g) if g > 0 => g,
                        _ => n as u32,
                    })
                    .collect();
                let sp: Vec<u32> = drivers
                    .iter()
                    .map(|d| cond.sprint_finish.get(&d.tla).copied().unwrap_or(0))
                    .collect();
                let sd: Vec<bool> = drivers
                    .iter()
                    .enumerate()
                    .map(|(i, d)| cond.sprint_dnf.contains(&d.tla) || sp[i] == 0)
                    .collect();
                Some((sq, sp, sd))
            } else if has_sprint {
                for (i, d) in drivers.iter().enumerate() {
                    scores[i] = d.quali_mu + normal(&mut rng) * d.quali_sd;
                }
                let sq = rank(&scores);
                let mut sd = vec![false; n];
                for (i, d) in drivers.iter().enumerate() {
                    // A sprint is about a third of a race distance; scale
                    // the retirement risk down accordingly.
                    sd[i] = rng.random::<f64>() < d.dnf_prob / 3.0;
                    scores[i] = d.race_mu + normal(&mut rng) * d.race_sd;
                    if sd[i] {
                        scores[i] += 1000.0;
                    }
                }
                let sp = rank(&scores);
                Some((sq, sp, sd))
            } else {
                None
            };

            for (i, d) in drivers.iter().enumerate() {
                let mut w = DriverWeekend {
                    quali_pos: quali_pos[i],
                    grid_pos: grid_pos[i],
                    finish_pos: race_pos[i],
                    dnf: dnf[i],
                    // Overtake points come from the fitted per-weekend rate.
                    // The rules award one point per overtake in the race and
                    // in the sprint alike, so the model books the whole
                    // weekend rate in the race leg.
                    overtakes: poisson(&mut rng, d.overtake_lambda),
                    fastest_lap: fl_winner == Some(i),
                    dotd: dotd_winner == Some(i),
                    ..Default::default()
                };
                if let Some((sq, sp, sd)) = &sprint {
                    w.has_sprint = true;
                    w.sprint_grid = sq[i];
                    w.sprint_pos = sp[i];
                    w.sprint_dnf = sd[i];
                }
                weekends[i] = w;
                let bd = self.rules.driver_breakdown(w);
                samples
                    .get_mut(&d.asset_id)
                    .expect("driver sample")
                    .push(f64::from(bd.total()));
                samples_nn
                    .get_mut(&d.asset_id)
                    .expect("driver sample")
                    .push(f64::from(bd.no_negative()));
            }
            for c in &self.constructors {
                let ws: Vec<DriverWeekend> = drivers
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| d.team_id == c.team_id)
                    .map(|(i, _)| weekends[i])
                    .collect();
                if ws.len() != 2 {
                    continue;
                }
                let score = self.rules.constructor_score(ws[0], ws[1], 0);
                samples
                    .get_mut(&c.asset_id)
                    .expect("constructor sample")
                    .push(f64::from(score.total) + c.pit_residual);
                samples_nn
                    .get_mut(&c.asset_id)
                    .expect("constructor sample")
                    .push(f64::from(score.no_negative) + c.pit_residual.max(0.0));
            }
        }

        let mut projections = Vec::new();
        let mut index = HashMap::new();
        let mut add = |id: &AssetId, name: &str, kind: AssetKind| {
            let sample = &samples[id];
            if sample.is_empty() {
                return;
            }
            index.insert(id.clone(), projections.len());
            projections.push(Projection {
                asset_id: id.clone(),
                name: name.to_string(),
                kind,
                dist: Distribution::of(sample),
                mean_no_negative: mean(&samples_nn[id]),
            });
        };
        for d in &drivers {
            add(&d.asset_id, &d.name, AssetKind::Driver);
        }
        for c in &self.constructors {
            add(&c.asset_id, &c.name, AssetKind::Constructor);
        }
        SimulationResult {
            round,
            has_sprint,
            sims,
            projections,
            index,
            samples,
            samples_no_negative: samples_nn,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::test_support::synthetic_season;
    use crate::rules::ScoringRules;
    use crate::shared::Tla;

    fn id(s: &str) -> AssetId {
        AssetId::new(s).unwrap()
    }

    fn r(n: u32) -> RoundNumber {
        RoundNumber::new(n).unwrap()
    }

    mod given_a_model_fitted_on_the_synthetic_season {
        use super::*;

        fn model() -> PaceModel {
            PaceModel::fit(&synthetic_season(), &ScoringRules::default(), Some(r(5)))
        }

        #[test]
        fn when_simulated_twice_with_one_seed_then_the_projections_repeat_exactly() {
            let m = model();
            let a = m.simulate(r(6), false, SimulationSettings::new(2000, 42));
            let b = m.simulate(r(6), false, SimulationSettings::new(2000, 42));
            assert_eq!(a.projections.len(), b.projections.len());
            for (x, y) in a.projections.iter().zip(&b.projections) {
                assert_eq!(x.dist.mean, y.dist.mean);
            }
        }

        #[test]
        fn when_the_next_round_is_simulated_then_every_asset_receives_a_projection() {
            let sim = model().simulate(r(6), false, SimulationSettings::new(4000, 1));
            assert_eq!(sim.projections.len(), 9);
        }

        #[test]
        fn when_the_next_round_is_simulated_then_the_fastest_driver_projects_above_the_slowest() {
            let sim = model().simulate(r(6), false, SimulationSettings::new(4000, 1));
            let fast = sim.by_id(&id("d0")).unwrap();
            let slow = sim.by_id(&id("d5")).unwrap();
            assert!(fast.dist.mean > slow.dist.mean);
        }

        #[test]
        fn when_the_next_round_is_simulated_then_the_percentiles_come_in_order() {
            let sim = model().simulate(r(6), false, SimulationSettings::new(4000, 1));
            for p in &sim.projections {
                assert!(p.dist.p10 <= p.dist.p50 && p.dist.p50 <= p.dist.p90);
            }
        }

        #[test]
        fn when_the_next_round_is_simulated_then_the_top_constructor_projects_above_the_bottom() {
            let sim = model().simulate(r(6), false, SimulationSettings::new(4000, 1));
            assert!(
                sim.by_id(&id("c0")).unwrap().dist.mean > sim.by_id(&id("c2")).unwrap().dist.mean
            );
        }

        #[test]
        fn when_the_round_has_a_sprint_then_the_front_runner_projects_more_points() {
            let m = model();
            let plain = m.simulate(r(6), false, SimulationSettings::new(4000, 1));
            let sprint = m.simulate(r(6), true, SimulationSettings::new(4000, 1));
            assert!(
                sprint.by_id(&id("d0")).unwrap().dist.mean
                    > plain.by_id(&id("d0")).unwrap().dist.mean
            );
        }

        #[test]
        fn when_the_slowest_driver_is_known_on_pole_then_its_projection_rises() {
            let m = model();
            let free = m.simulate(r(6), false, SimulationSettings::new(4000, 1));
            let mut cond = WeekendConditions::default();
            let order: Vec<Tla> = ["CCB", "AAA", "AAB", "BBA", "BBB", "CCA"]
                .iter()
                .map(|s| Tla::parse(s).unwrap())
                .collect();
            cond.quali = WeekendConditions::positions(&order);
            let known = m.simulate_with(r(6), false, SimulationSettings::new(4000, 1), &cond);
            assert!(
                known.by_id(&id("d5")).unwrap().dist.mean
                    > free.by_id(&id("d5")).unwrap().dist.mean
            );
        }

        #[test]
        fn when_the_fastest_driver_starts_from_the_back_then_its_projection_falls() {
            let m = model();
            let free = m.simulate(r(6), false, SimulationSettings::new(4000, 1));
            let mut cond = WeekendConditions::default();
            cond.back_of_grid.insert(Tla::parse("AAA").unwrap());
            let back = m.simulate_with(r(6), false, SimulationSettings::new(4000, 1), &cond);
            assert!(
                back.by_id(&id("d0")).unwrap().dist.mean < free.by_id(&id("d0")).unwrap().dist.mean
            );
        }
    }

    #[test]
    fn given_scores_with_a_tie_when_ranked_then_the_earlier_index_wins() {
        assert_eq!(rank(&[2.0, 1.0, 2.0]), vec![2, 1, 3]);
    }
}
