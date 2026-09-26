//! The route handlers.
//!
//! Every handler reads the shared state, builds a use case input, and
//! runs the use case. CPU-bound use cases run on the blocking pool so
//! the async runtime stays responsive.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use overcut_application::dto::{
    ConditionsInput, OptimizeInput, ProjectInput, ReviewInput, RulesDto,
};
use overcut_application::usecases::{
    Hindsight, OptimizeTeam, PredictPrices, ProjectRound, ReviewRound, RunBacktest, SeasonOverview,
};
use serde::Serialize;

use super::error::ApiError;
use super::AppState;
use crate::text::split_list;

/// The shared state handle.
pub type Shared = State<Arc<AppState>>;

/// The query string as a plain map.
pub type Params = Query<HashMap<String, String>>;

/// Reads an integer query parameter. A missing or malformed value gives
/// the default, as in the original server.
fn query_int<T: FromStr>(q: &HashMap<String, String>, name: &str, default: T) -> T {
    q.get(name)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

/// Reads a comma-separated query parameter.
fn query_list(q: &HashMap<String, String>, name: &str) -> Vec<String> {
    q.get(name).map(|v| split_list(v)).unwrap_or_default()
}

/// Serialises a view as JSON.
fn json<T: Serialize>(view: T) -> Response {
    Json(view).into_response()
}

/// Runs a CPU-bound use case on the blocking pool.
async fn blocking<T, F>(state: &Arc<AppState>, f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&AppState) -> T + Send + 'static,
{
    let state = Arc::clone(state);
    Ok(tokio::task::spawn_blocking(move || f(&state)).await?)
}

/// `GET /api/season`: the calendar and every asset.
pub async fn season(State(state): Shared) -> Result<Response, ApiError> {
    let view = blocking(&state, |s| SeasonOverview::new(&s.ctx).execute(Utc::now())).await?;
    Ok(json(view))
}

/// `GET /api/rules`: the active scoring rules.
pub async fn rules(State(state): Shared) -> Response {
    json(RulesDto::from(state.ctx.rules()))
}

/// `GET /api/project`: one round's projection.
pub async fn project(State(state): Shared, Query(q): Params) -> Result<Response, ApiError> {
    let input = ProjectInput {
        round: query_int(&q, "round", 0),
        sims: query_int(&q, "sims", 0),
        seed: query_int(&q, "seed", 0),
        conditions: ConditionsInput {
            quali: query_list(&q, "quali"),
            grid: query_list(&q, "grid"),
            back: query_list(&q, "back"),
            fp3: query_list(&q, "fp3"),
            ..ConditionsInput::default()
        },
    };
    let view = blocking(&state, move |s| ProjectRound::new(&s.ctx).execute(input)).await??;
    Ok(json(view))
}

/// `POST /api/optimize`: the best teams for a round.
pub async fn optimize(
    State(state): Shared,
    body: Result<Json<OptimizeInput>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(input) = body?;
    let view = blocking(&state, move |s| OptimizeTeam::new(&s.ctx).execute(input)).await??;
    Ok(json(view))
}

/// `GET /api/prices`: the predicted price changes.
pub async fn prices(State(state): Shared) -> Result<Response, ApiError> {
    let view = blocking(&state, |s| PredictPrices::new(&s.ctx).execute()).await?;
    Ok(json(view))
}

/// `GET /api/backtest`: the walk-forward backtest.
pub async fn backtest(State(state): Shared, Query(q): Params) -> Result<Response, ApiError> {
    let sims = query_int(&q, "sims", 3000);
    let view = blocking(&state, move |s| RunBacktest::new(&s.ctx).execute(sims)).await?;
    Ok(json(view))
}

/// `GET /api/hindsight`: the best teams for a finished round.
pub async fn hindsight(State(state): Shared, Query(q): Params) -> Result<Response, ApiError> {
    let round = query_int(&q, "round", 0);
    let top = query_int(&q, "top", 5);
    let view = blocking(&state, move |s| Hindsight::new(&s.ctx).execute(round, top)).await??;
    Ok(json(view))
}

/// `POST /api/review`: a finished round's projection against the
/// official points.
pub async fn review(
    State(state): Shared,
    body: Result<Json<ReviewInput>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(input) = body?;
    let view = blocking(&state, move |s| {
        ReviewRound::new(&s.ctx).execute(input, Utc::now())
    })
    .await??;
    Ok(json(view))
}

/// `POST /api/sync`: download the season again and return the overview.
pub async fn sync(State(state): Shared) -> Result<Response, ApiError> {
    let (_, season) = state.sync.execute(state.season, false).await?;
    if let Some(season) = season {
        tracing::info!(path = %state.store.path().display(), "season synced");
        state.ctx.replace_season(season);
    }
    let view = blocking(&state, |s| SeasonOverview::new(&s.ctx).execute(Utc::now())).await?;
    Ok(json(view))
}

/// `GET /healthz`: liveness.
pub async fn healthz() -> Response {
    json(serde_json::json!({ "status": "ok" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_malformed_integer_when_read_then_the_default_is_used() {
        let q: HashMap<String, String> = [("sims".to_string(), "abc".to_string())]
            .into_iter()
            .collect();
        assert_eq!(query_int(&q, "sims", 3000usize), 3000);
        assert_eq!(query_int(&q, "round", 0u32), 0);
    }

    #[test]
    fn given_a_valid_integer_when_read_then_it_is_parsed() {
        let q: HashMap<String, String> = [("top".to_string(), " 7 ".to_string())]
            .into_iter()
            .collect();
        assert_eq!(query_int(&q, "top", 5usize), 7);
    }

    #[test]
    fn given_a_comma_list_when_read_then_it_is_split() {
        let q: HashMap<String, String> = [("quali".to_string(), "VER, NOR,".to_string())]
            .into_iter()
            .collect();
        assert_eq!(query_list(&q, "quali"), vec!["VER", "NOR"]);
        assert!(query_list(&q, "grid").is_empty());
    }
}
