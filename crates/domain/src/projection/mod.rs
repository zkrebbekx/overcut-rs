//! The projection model: a fitted pace model and a Monte Carlo simulation
//! of one round.
//!
//! The fit uses two independent sources and calibrates one against the
//! other:
//!
//! - The classification source supplies the qualifying, sprint, and race
//!   classifications. The fit turns them into a per-driver pace estimate
//!   with an exponentially weighted moving average (EWMA), so recent form
//!   counts more than old form.
//! - The fantasy feed supplies the true fantasy points per asset per
//!   gameday. The fit uses them to estimate the components that public
//!   timing data cannot derive: overtake points, driver-of-the-day votes,
//!   and the constructor pit-stop component. The pit-stop component is the
//!   EWMA of the residual between official constructor points and
//!   rule-derived points, so a small error in a scoring table corrects
//!   itself from the official numbers.

mod conditions;
mod fit;
mod simulate;
mod stats;

pub use conditions::WeekendConditions;
pub use fit::{ConstructorPace, DriverPace, PaceModel, HALF_LIFE};
pub use simulate::{Projection, SimulationResult, SimulationSettings, DEFAULT_GRID_INFLUENCE};
pub use stats::{mean, Distribution};

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod fit_tests {
    use super::test_support::synthetic_season;
    use super::*;
    use crate::rules::ScoringRules;
    use crate::shared::{RoundNumber, Tla};

    fn tla(s: &str) -> Tla {
        Tla::parse(s).unwrap()
    }

    mod given_a_synthetic_five_round_season_with_a_fixed_pecking_order {
        use super::*;

        #[test]
        fn when_the_model_fits_on_the_full_season_then_every_selectable_asset_gets_a_model() {
            let m = PaceModel::fit(
                &synthetic_season(),
                &ScoringRules::default(),
                RoundNumber::new(5).ok(),
            );
            assert_eq!(m.drivers.len(), 6);
            assert_eq!(m.constructors.len(), 3);
        }

        #[test]
        fn when_the_model_fits_then_the_fastest_driver_carries_the_lowest_race_mean() {
            let m = PaceModel::fit_all(&synthetic_season(), &ScoringRules::default());
            let fast = m.driver_by_tla(&tla("AAA")).unwrap();
            let slow = m.driver_by_tla(&tla("CCB")).unwrap();
            assert!(fast.race_mu < slow.race_mu);
            assert!((fast.race_mu - 1.0).abs() < 0.01);
        }

        #[test]
        fn when_a_driver_never_retired_then_it_keeps_a_small_shrunk_dnf_rate() {
            let m = PaceModel::fit_all(&synthetic_season(), &ScoringRules::default());
            let d = m.driver_by_tla(&tla("AAA")).unwrap();
            assert!(d.dnf_prob > 0.0 && d.dnf_prob < 0.2);
        }

        #[test]
        fn when_one_driver_retires_in_every_round_then_its_dnf_rate_rises_far_above_the_field() {
            let season = synthetic_season();
            let mut rounds = season.rounds().to_vec();
            for r in &mut rounds {
                r.race.get_mut(&tla("CCB")).unwrap().dnf = true;
            }
            let season = crate::season::Season::new(
                2026,
                season.synced_at(),
                rounds,
                season.assets().to_vec(),
            )
            .unwrap();
            let m = PaceModel::fit_all(&season, &ScoringRules::default());
            let crash = m.driver_by_tla(&tla("CCB")).unwrap();
            let clean = m.driver_by_tla(&tla("AAA")).unwrap();
            assert!(crash.dnf_prob > 0.4);
            assert!(crash.dnf_prob > clean.dnf_prob * 3.0);
        }

        #[test]
        fn when_the_model_fits_on_no_rounds_then_every_driver_takes_the_prior() {
            let m = PaceModel::fit(&synthetic_season(), &ScoringRules::default(), None);
            for d in &m.drivers {
                assert_eq!(d.quali_mu, 11.0);
                assert_eq!(d.rounds, 0);
            }
        }
    }
}
