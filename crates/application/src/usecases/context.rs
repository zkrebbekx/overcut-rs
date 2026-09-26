//! The shared state of the analysis use cases.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use overcut_domain::backtest::{self, BacktestReport};
use overcut_domain::projection::{
    PaceModel, SimulationResult, SimulationSettings, WeekendConditions,
};
use overcut_domain::rules::ScoringRules;
use overcut_domain::season::{Round, Season};

use crate::dto::ConditionsInput;

use super::{DEFAULT_BACKTEST_SIMS, DEFAULT_SEED, DEFAULT_SIMS};

struct State {
    season: Arc<Season>,
    simulations: HashMap<String, Arc<SimulationResult>>,
    backtests: HashMap<usize, Arc<BacktestReport>>,
}

/// The season, the rules, and the caches of expensive results.
///
/// The context is safe to share between threads. A sync replaces the
/// season and clears every cache. Simulations run outside the lock, so a
/// long computation never blocks a reader.
pub struct AnalysisContext {
    rules: ScoringRules,
    grid_influence: f64,
    state: RwLock<State>,
}

impl AnalysisContext {
    /// Builds a context over a season with the given rules.
    pub fn new(season: Season, rules: ScoringRules) -> Self {
        Self {
            rules,
            grid_influence: overcut_domain::projection::DEFAULT_GRID_INFLUENCE,
            state: RwLock::new(State {
                season: Arc::new(season),
                simulations: HashMap::new(),
                backtests: HashMap::new(),
            }),
        }
    }

    /// Overrides the grid influence of every simulation (a calibration
    /// knob).
    #[must_use]
    pub fn with_grid_influence(mut self, grid_influence: f64) -> Self {
        self.grid_influence = grid_influence;
        self
    }

    /// The scoring rules.
    pub fn rules(&self) -> &ScoringRules {
        &self.rules
    }

    /// The current season.
    pub fn season(&self) -> Arc<Season> {
        Arc::clone(&self.state.read().expect("context lock").season)
    }

    /// Replaces the season and clears every cache.
    pub fn replace_season(&self, season: Season) {
        let mut state = self.state.write().expect("context lock");
        state.season = Arc::new(season);
        state.simulations.clear();
        state.backtests.clear();
    }

    /// Resolves the settings, replacing zero with the defaults.
    pub fn settings(&self, sims: usize, seed: u64) -> SimulationSettings {
        SimulationSettings {
            sims: if sims == 0 { DEFAULT_SIMS } else { sims },
            seed: if seed == 0 { DEFAULT_SEED } else { seed },
            grid_influence: self.grid_influence,
        }
    }

    /// Returns a cached or fresh simulation of the round under the given
    /// conditions. The model fits on the rounds before the target.
    pub fn simulate(
        &self,
        target: &Round,
        settings: SimulationSettings,
        input: &ConditionsInput,
        cond: &WeekendConditions,
    ) -> Arc<SimulationResult> {
        let key = format!(
            "{}|{}|{}|{}",
            target.number,
            settings.sims,
            settings.seed,
            serde_json::to_string(input).unwrap_or_default()
        );
        let season = {
            let state = self.state.read().expect("context lock");
            if let Some(hit) = state.simulations.get(&key) {
                return Arc::clone(hit);
            }
            Arc::clone(&state.season)
        };
        let model = PaceModel::fit(&season, &self.rules, target.number.previous());
        let result =
            Arc::new(model.simulate_with(target.number, target.has_sprint, settings, cond));
        let mut state = self.state.write().expect("context lock");
        // A sync may have replaced the season while we computed: only cache
        // against the season we used.
        if Arc::ptr_eq(&state.season, &season) {
            state.simulations.insert(key, Arc::clone(&result));
        }
        result
    }

    /// Returns a cached or fresh backtest with the given simulation count
    /// per round. Zero means the default.
    pub fn backtest(&self, sims: usize) -> Arc<BacktestReport> {
        let sims = if sims == 0 {
            DEFAULT_BACKTEST_SIMS
        } else {
            sims
        };
        let season = {
            let state = self.state.read().expect("context lock");
            if let Some(hit) = state.backtests.get(&sims) {
                return Arc::clone(hit);
            }
            Arc::clone(&state.season)
        };
        let settings = SimulationSettings {
            sims,
            seed: DEFAULT_SEED,
            grid_influence: self.grid_influence,
        };
        let report = Arc::new(backtest::run(&season, &self.rules, settings));
        let mut state = self.state.write().expect("context lock");
        if Arc::ptr_eq(&state.season, &season) {
            state.backtests.insert(sims, Arc::clone(&report));
        }
        report
    }
}
