//! The overcut engine for the browser.
//!
//! Every function takes and returns JSON text, so the JavaScript side
//! stays thin and the same worker can drive the Go and the Rust build.
//! A call that fails returns `{"error": "<message>"}`.
//!
//! ```text
//! load(seasonJSON)          -> "" or an error message
//! season()                  -> SeasonView
//! rules()                   -> RulesDto
//! project(inputJSON)        -> ProjectionView
//! optimize(inputJSON)       -> OptimizeView
//! prices()                  -> PricesView
//! backtest(sims)            -> BacktestView
//! hindsight(round, top)     -> HindsightView
//! review(inputJSON)         -> ReviewView
//! ```
//!
//! WebAssembly runs on one thread, so the engine lives in a thread-local
//! cell. A call before `load` returns an error.

use std::cell::RefCell;

use chrono::{DateTime, Utc};
use overcut_application::dto::{OptimizeInput, ProjectInput, ReviewInput, RulesDto};
use overcut_application::usecases::{
    Hindsight, OptimizeTeam, PredictPrices, ProjectRound, ReviewRound, RunBacktest, SeasonOverview,
};
use overcut_application::AnalysisContext;
use overcut_domain::rules::ScoringRules;
use serde::de::DeserializeOwned;
use serde::Serialize;
use wasm_bindgen::prelude::*;

thread_local! {
    static ENGINE: RefCell<Option<AnalysisContext>> = const { RefCell::new(None) };
}

/// The current time from the JavaScript clock. `chrono` cannot read the
/// clock on `wasm32-unknown-unknown` without extra bindings.
fn now() -> DateTime<Utc> {
    DateTime::from_timestamp_millis(js_sys::Date::now() as i64).unwrap_or_default()
}

/// Encodes a result, or an error object, as JSON text.
fn reply<T: Serialize, E: std::fmt::Display>(result: Result<T, E>) -> String {
    let encoded = match result {
        Ok(v) => serde_json::to_string(&v).map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    encoded.unwrap_or_else(|message| serde_json::json!({ "error": message }).to_string())
}

/// Runs `f` against the loaded engine.
fn with_engine<T: Serialize>(f: impl FnOnce(&AnalysisContext) -> Result<T, String>) -> String {
    ENGINE.with(|cell| match cell.borrow().as_ref() {
        Some(ctx) => reply(f(ctx)),
        None => reply::<T, _>(Err("engine not loaded; call load(seasonJSON) first")),
    })
}

/// Parses a JSON input.
fn parse<T: DeserializeOwned>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| format!("bad input: {e}"))
}

/// Loads a season from JSON text with the default rules. Returns an empty
/// string on success or the error message.
#[wasm_bindgen]
pub fn load(season_json: &str) -> String {
    match overcut_codec::decode(season_json) {
        Ok(season) => {
            ENGINE.with(|cell| {
                *cell.borrow_mut() = Some(AnalysisContext::new(season, ScoringRules::default()));
            });
            String::new()
        }
        Err(e) => e.to_string(),
    }
}

/// The calendar and every asset.
#[wasm_bindgen]
pub fn season() -> String {
    with_engine(|ctx| Ok(SeasonOverview::new(ctx).execute(now())))
}

/// The active scoring rules.
#[wasm_bindgen]
pub fn rules() -> String {
    with_engine(|ctx| Ok(RulesDto::from(ctx.rules())))
}

/// Projects a round. The input is a `ProjectInput` as JSON.
#[wasm_bindgen]
pub fn project(input_json: &str) -> String {
    with_engine(|ctx| {
        let input: ProjectInput = parse(input_json)?;
        ProjectRound::new(ctx)
            .execute(input)
            .map_err(|e| e.to_string())
    })
}

/// Finds the best team. The input is an `OptimizeInput` as JSON.
#[wasm_bindgen]
pub fn optimize(input_json: &str) -> String {
    with_engine(|ctx| {
        let input: OptimizeInput = parse(input_json)?;
        OptimizeTeam::new(ctx)
            .execute(input)
            .map_err(|e| e.to_string())
    })
}

/// Predicts the next price change per asset.
#[wasm_bindgen]
pub fn prices() -> String {
    with_engine(|ctx| Ok(PredictPrices::new(ctx).execute()))
}

/// Runs the walk-forward backtest with `sims` simulations per round.
#[wasm_bindgen]
pub fn backtest(sims: u32) -> String {
    with_engine(|ctx| Ok(RunBacktest::new(ctx).execute(sims as usize)))
}

/// The best teams for a finished round. Zero means the latest.
#[wasm_bindgen]
pub fn hindsight(round: u32, top: u32) -> String {
    with_engine(|ctx| {
        Hindsight::new(ctx)
            .execute(round, top as usize)
            .map_err(|e| e.to_string())
    })
}

/// Reviews a finished round. The input is a `ReviewInput` as JSON.
#[wasm_bindgen]
pub fn review(input_json: &str) -> String {
    with_engine(|ctx| {
        let input: ReviewInput = parse(input_json)?;
        ReviewRound::new(ctx)
            .execute(input, now())
            .map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/season2026.json"
    );

    fn json(s: &str) -> serde_json::Value {
        serde_json::from_str(s).unwrap()
    }

    mod given_no_engine {
        use super::*;

        #[test]
        fn when_a_call_is_made_then_the_reply_is_an_error_object() {
            let v = json(&hindsight(12, 1));
            assert!(v["error"].as_str().unwrap().contains("load"));
        }
    }

    mod given_the_fixture_is_loaded {
        use super::*;

        fn load_fixture() {
            assert_eq!(load(&std::fs::read_to_string(FIXTURE).unwrap()), "");
        }

        #[test]
        fn when_hindsight_runs_then_the_reply_is_the_view() {
            load_fixture();
            let v = json(&hindsight(12, 1));
            assert_eq!(v["round"], 12);
            assert_eq!(v["teams"][0]["score"], 295.0);
        }

        #[test]
        fn when_the_input_is_malformed_then_the_reply_names_the_problem() {
            load_fixture();
            let v = json(&optimize("{not json"));
            assert!(v["error"].as_str().unwrap().contains("bad input"));
        }

        #[test]
        fn when_the_season_json_is_malformed_then_load_returns_the_message() {
            assert!(load("{").contains("parse"));
        }
    }
}
