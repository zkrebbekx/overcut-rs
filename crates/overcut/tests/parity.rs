//! Parity tests: the Rust engine against numbers the Go binary produced on
//! the committed 2026 season (`fixtures/season2026.json`).
//!
//! The deterministic parts (rules, optimizer, price model, baselines) must
//! match exactly. The Monte Carlo parts use a different random stream, so
//! they must match inside a statistical tolerance.

use std::path::PathBuf;

use overcut_application::dto::{OptimizeInput, ProjectInput, ReviewInput};
use overcut_application::usecases::{
    Hindsight, OptimizeTeam, PredictPrices, ProjectRound, ReviewRound, RunBacktest,
};
use overcut_application::AnalysisContext;
use overcut_domain::rules::{DriverWeekend, ScoringRules};
use overcut_domain::season::{Season, SeasonRepository};
use overcut_infrastructure::JsonSeasonRepository;

fn fixture() -> Season {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/season2026.json");
    JsonSeasonRepository::new(path)
        .load()
        .expect("fixture loads")
        .expect("fixture exists")
}

fn ctx() -> AnalysisContext {
    AnalysisContext::new(fixture(), ScoringRules::default())
}

fn close(actual: f64, expected: f64, tol: f64) {
    assert!(
        (actual - expected).abs() <= tol,
        "expected {expected} ± {tol}, got {actual}"
    );
}

mod given_the_committed_2026_season {
    use super::*;

    #[test]
    fn when_loaded_then_it_has_23_rounds_14_finished_and_35_assets() {
        let s = fixture();
        assert_eq!(s.rounds().len(), 23);
        assert_eq!(s.completed_rounds().count(), 14);
        assert_eq!(s.assets().len(), 35);
        assert_eq!(s.next_round().map(|r| r.number.get()), Some(15));
    }

    #[test]
    fn when_the_rules_score_qualifying_then_they_reproduce_every_official_qualifying_point() {
        let s = fixture();
        let rules = ScoringRules::default();
        let mut checked = 0;
        let mut mismatches = Vec::new();
        for r in s.completed_rounds() {
            for a in s.drivers() {
                let (Some(tla), Some(h)) = (&a.tla, a.at(r.number.gameday())) else {
                    continue;
                };
                let Some(&pos) = r.quali.get(tla) else {
                    continue;
                };
                let pts = rules.driver_points(DriverWeekend {
                    quali_pos: pos,
                    ..Default::default()
                });
                checked += 1;
                if f64::from(pts) != h.quali_pts {
                    mismatches.push((r.number.get(), tla.to_string(), pts, h.quali_pts));
                }
            }
        }
        // Three driver-rounds carry a stewards' decision that the
        // classification alone cannot show (a no-time and two penalties).
        assert!(checked >= 300, "checked {checked}");
        assert!(mismatches.len() <= 3, "{mismatches:?}");
    }

    #[test]
    fn when_hindsight_runs_for_round_12_then_it_matches_the_go_optimum() {
        let view = Hindsight::new(&ctx()).execute(12, 3).unwrap();
        assert_eq!(view.name, "Dutch Grand Prix");
        let best = &view.teams[0];
        close(best.score, 295.0, 1e-9);
        close(best.cost, 99.7, 1e-6);
        let captain = best
            .drivers
            .iter()
            .find(|d| d.id == best.captain_id)
            .unwrap();
        assert_eq!(captain.name, "Lando Norris");
        let names: Vec<&str> = best.constructors.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["McLaren", "Ferrari"]);
        close(view.teams[1].score, 289.0, 1e-9);
        close(view.teams[2].score, 288.0, 1e-9);
    }

    #[test]
    fn when_hindsight_runs_for_round_14_then_it_matches_the_go_optimum_including_the_tie_order() {
        let view = Hindsight::new(&ctx()).execute(14, 3).unwrap();
        close(view.teams[0].score, 199.0, 1e-9);
        close(view.teams[0].cost, 99.8, 1e-6);
        close(view.teams[1].score, 199.0, 1e-9);
        close(view.teams[1].cost, 100.0, 1e-6);
        close(view.teams[2].score, 198.0, 1e-9);
    }

    #[test]
    fn when_prices_are_predicted_then_the_walk_forward_report_matches_go() {
        let view = PredictPrices::new(&ctx()).execute();
        close(view.report.mae, 0.274, 0.0006);
        close(view.report.naive_mae, 0.373, 0.0006);
        assert_eq!(view.report.moves, 378);
        close(view.report.direction, 0.80, 0.006);
        close(view.predictions[0].change, 0.30, 1e-9);
        let mercedes = view
            .predictions
            .iter()
            .find(|p| p.name == "Mercedes")
            .unwrap();
        close(mercedes.change, 0.30, 1e-9);
    }

    #[test]
    fn when_round_15_is_projected_then_the_means_sit_close_to_the_go_means() {
        let view = ProjectRound::new(&ctx())
            .execute(ProjectInput {
                sims: 20_000,
                seed: 1,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(view.round, 15);
        assert_eq!(view.name, "Azerbaijan Grand Prix");
        assert!(view.quali_from_data && view.grid_from_data);
        let mean = |name: &str| view.assets.iter().find(|a| a.name == name).unwrap().mean;
        // Go engine, GET /api/project?sims=20000&seed=1 on the same file.
        close(mean("Mercedes"), 63.4, 1.5);
        close(mean("Ferrari"), 58.0, 1.5);
        close(mean("McLaren"), 53.3, 1.5);
        close(mean("Kimi Antonelli"), 27.8, 1.5);
        close(mean("George Russell"), 25.1, 1.5);
        let top: Vec<&str> = view
            .assets
            .iter()
            .take(4)
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(
            top,
            vec!["Mercedes", "Ferrari", "McLaren", "Red Bull Racing"]
        );
    }

    #[test]
    fn when_round_15_is_optimized_then_the_best_team_scores_close_to_go_and_values_every_chip() {
        let view = OptimizeTeam::new(&ctx())
            .execute(OptimizeInput {
                sims: 20_000,
                seed: 1,
                top: 3,
                ..Default::default()
            })
            .unwrap();
        // Go engine, POST /api/optimize {"sims":20000,"seed":1,"top":3}.
        let best = &view.teams[0];
        close(best.score, 183.4, 2.0);
        close(best.cost, 99.9, 1e-6);
        let captain = best
            .drivers
            .iter()
            .find(|d| d.id == best.captain_id)
            .unwrap();
        let top_points = best
            .drivers
            .iter()
            .map(|d| d.points)
            .fold(f64::NEG_INFINITY, f64::max);
        close(captain.points, top_points, 1e-9);
        close(best.p50, 186.2, 3.0);
        let gain = |chip: &str| view.chips.iter().find(|c| c.chip == chip).unwrap().gain;
        close(gain("limitless"), 88.8, 3.0);
        close(gain("nonegative"), 38.5, 3.0);
        close(gain("3x"), 37.0, 3.0);
        close(gain("autopilot"), 5.0, 1.5);
        let chips: Vec<&str> = view.chips.iter().map(|c| c.chip.as_str()).collect();
        for chip in [
            "nonegative",
            "3x",
            "autopilot",
            "wildcard",
            "limitless",
            "finalfix",
        ] {
            assert!(chips.contains(&chip), "missing chip {chip}");
        }
        assert!(view.teams[0].p10 < view.teams[0].p50 && view.teams[0].p50 < view.teams[0].p90);
    }

    #[test]
    fn when_round_14_is_reviewed_with_the_go_team_then_the_actuals_match_exactly() {
        let ctx = ctx();
        let season = ctx.season();
        let team = overcut_application::usecases::resolve_team(
            &season,
            &["GAS", "COL", "HUL", "ANT", "LIN", "Mercedes", "Ferrari"].map(String::from),
        )
        .unwrap();
        let view = ReviewRound::new(&ctx)
            .execute(
                ReviewInput {
                    round: 14,
                    sims: 20_000,
                    team: team.iter().map(ToString::to_string).collect(),
                    ..Default::default()
                },
                chrono::Utc::now(),
            )
            .unwrap();
        close(view.hindsight_points, 199.0, 1e-9);
        close(view.team_actual, 199.0, 1e-9);
        close(view.team_projected, 228.4, 6.0);
        close(view.driver_mae, 8.3, 1.5);
        close(view.coverage, 0.82, 0.12);
        assert_eq!(view.assets[0].name, "Ferrari");
    }

    #[test]
    fn when_the_backtest_runs_then_the_deterministic_baselines_match_go_and_the_model_is_close() {
        let view = RunBacktest::new(&ctx()).execute(1_000);
        assert_eq!(view.rounds.len(), 11);
        close(view.baseline_prev, 15.5, 0.05);
        close(view.baseline_season, 11.8, 0.05);
        close(view.hindsight_team_pts, 294.0, 0.5);
        close(view.naive_team_pts, 141.0, 0.5);
        close(view.driver_mae, 11.2, 1.0);
        close(view.grid_driver_mae, 11.0, 1.0);
        assert!(view.driver_mae < view.baseline_prev);
        close(view.coverage, 0.78, 0.06);
    }
}
