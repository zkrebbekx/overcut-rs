//! The local JSON API.
//!
//! The routes match the original Go server, so the existing web client
//! works unchanged:
//!
//! | Method | Path             | Use case        |
//! |--------|------------------|-----------------|
//! | GET    | `/api/season`    | `SeasonOverview` |
//! | GET    | `/api/rules`     | the active rules |
//! | GET    | `/api/project`   | `ProjectRound`   |
//! | POST   | `/api/optimize`  | `OptimizeTeam`   |
//! | GET    | `/api/prices`    | `PredictPrices`  |
//! | GET    | `/api/backtest`  | `RunBacktest`    |
//! | GET    | `/api/hindsight` | `Hindsight`      |
//! | POST   | `/api/review`    | `ReviewRound`    |
//! | POST   | `/api/sync`      | `SyncSeason`     |
//! | GET    | `/healthz`       | liveness         |
//!
//! With a UI directory, every other path serves the static app and falls
//! back to `index.html` for client routes.

pub mod error;
pub mod routes;

use std::path::Path;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use overcut_application::usecases::SyncSeason;
use overcut_application::AnalysisContext;
use overcut_infrastructure::JsonSeasonRepository;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

/// The state every handler shares.
pub struct AppState {
    /// The season, the rules, and the caches.
    pub ctx: AnalysisContext,
    /// The season store on disk.
    pub store: Arc<JsonSeasonRepository>,
    /// The sync use case over the public sources.
    pub sync: SyncSeason,
    /// The season year to sync.
    pub season: u16,
}

/// Builds the API router. With `ui`, the router also serves the static
/// app from that directory.
pub fn router(state: Arc<AppState>, ui: Option<&Path>) -> Router {
    let mut app = Router::new()
        .route("/api/season", get(routes::season))
        .route("/api/rules", get(routes::rules))
        .route("/api/project", get(routes::project))
        .route("/api/optimize", post(routes::optimize))
        .route("/api/prices", get(routes::prices))
        .route("/api/backtest", get(routes::backtest))
        .route("/api/hindsight", get(routes::hindsight))
        .route("/api/review", post(routes::review))
        .route("/api/sync", post(routes::sync))
        .route("/healthz", get(routes::healthz));
    if let Some(dir) = ui {
        let index = ServeFile::new(dir.join("index.html"));
        app = app.fallback_service(ServeDir::new(dir).fallback(index));
    }
    app.layer(TraceLayer::new_for_http()).with_state(state)
}

#[cfg(test)]
mod tests {
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Request, StatusCode};
    use chrono::Utc;
    use overcut_domain::rules::ScoringRules;
    use overcut_domain::season::{Asset, GamedaySnapshot, RaceRow, Round, Season};
    use overcut_domain::shared::{AssetId, AssetKind, Gameday, RoundNumber, TeamId, Tla};
    use tower::ServiceExt;

    use super::*;
    use crate::cli::sync::use_case;

    fn tla(s: &str) -> Tla {
        Tla::parse(s).unwrap()
    }

    fn snap(g: u32, price: f64, points: f64) -> GamedaySnapshot {
        GamedaySnapshot {
            points,
            ownership: 10.0,
            ..GamedaySnapshot::new(Gameday::new(g).unwrap(), price)
        }
    }

    fn driver(id: &str, code: &str, team: &str, price: f64) -> Asset {
        Asset {
            id: AssetId::new(id).unwrap(),
            kind: AssetKind::Driver,
            name: format!("Driver {code}"),
            tla: Some(tla(code)),
            team_id: TeamId::new(team).unwrap(),
            team_name: team.to_uppercase(),
            history: vec![snap(1, price, 20.0), snap(2, price, 0.0)],
        }
    }

    fn constructor(id: &str, team: &str, price: f64) -> Asset {
        Asset {
            id: AssetId::new(id).unwrap(),
            kind: AssetKind::Constructor,
            name: team.to_uppercase(),
            tla: None,
            team_id: TeamId::new(team).unwrap(),
            team_name: String::new(),
            history: vec![snap(1, price, 30.0), snap(2, price, 0.0)],
        }
    }

    /// Two rounds: round 1 raced, round 2 pending. Four drivers and two
    /// constructors with two gamedays each.
    fn tiny_season() -> Season {
        let mut r1 = Round::new(RoundNumber::new(1).unwrap(), "Test Grand Prix");
        r1.has_results = true;
        r1.quali = [
            (tla("AAA"), 1),
            (tla("BBB"), 2),
            (tla("CCC"), 3),
            (tla("DDD"), 4),
        ]
        .into_iter()
        .collect();
        r1.race = [
            (
                tla("AAA"),
                RaceRow {
                    grid: 1,
                    pos: 1,
                    ..Default::default()
                },
            ),
            (
                tla("BBB"),
                RaceRow {
                    grid: 2,
                    pos: 2,
                    ..Default::default()
                },
            ),
            (
                tla("CCC"),
                RaceRow {
                    grid: 3,
                    pos: 3,
                    ..Default::default()
                },
            ),
            (
                tla("DDD"),
                RaceRow {
                    grid: 4,
                    pos: 4,
                    dnf: true,
                    ..Default::default()
                },
            ),
        ]
        .into_iter()
        .collect();
        let r2 = Round::new(RoundNumber::new(2).unwrap(), "Second Grand Prix");
        Season::new(
            2026,
            Utc::now(),
            vec![r1, r2],
            vec![
                driver("d1", "AAA", "t1", 30.0),
                driver("d2", "BBB", "t1", 25.0),
                driver("d3", "CCC", "t2", 20.0),
                driver("d4", "DDD", "t2", 10.0),
                constructor("c1", "t1", 25.0),
                constructor("c2", "t2", 15.0),
            ],
        )
        .unwrap()
    }

    fn app() -> (Router, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(JsonSeasonRepository::new(dir.path().join("season.json")));
        let state = AppState {
            ctx: AnalysisContext::new(tiny_season(), ScoringRules::default()),
            sync: use_case(Arc::clone(&store)),
            store,
            season: 2026,
        };
        (router(Arc::new(state), None), dir)
    }

    async fn get_json(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let res = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).expect("json body"))
    }

    async fn post_json(app: Router, uri: &str, body: &str) -> (StatusCode, serde_json::Value) {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).expect("json body"))
    }

    #[tokio::test]
    async fn given_the_api_when_healthz_is_requested_then_status_is_ok() {
        let (app, _dir) = app();
        let (status, body) = get_json(app, "/healthz").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn given_default_rules_when_rules_are_requested_then_the_budget_is_100() {
        let (app, _dir) = app();
        let (status, body) = get_json(app, "/api/rules").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["budget"], 100.0);
    }

    #[tokio::test]
    async fn given_the_tiny_season_when_the_season_is_requested_then_two_rounds_and_six_assets_return(
    ) {
        let (app, _dir) = app();
        let (status, body) = get_json(app, "/api/season").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["rounds"].as_array().unwrap().len(), 2);
        assert_eq!(body["assets"].as_array().unwrap().len(), 6);
        assert_eq!(body["next_round"], 2);
    }

    #[tokio::test]
    async fn given_an_unknown_round_when_hindsight_is_requested_then_400_with_an_error_field() {
        let (app, _dir) = app();
        let (status, body) = get_json(app, "/api/hindsight?round=99").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("99"));
    }

    #[tokio::test]
    async fn given_the_finished_round_when_hindsight_is_requested_then_teams_return() {
        let (app, _dir) = app();
        let (status, body) = get_json(app, "/api/hindsight?round=1&top=2").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["round"], 1);
        assert!(body["teams"].is_array());
    }

    #[tokio::test]
    async fn given_the_tiny_season_when_a_projection_is_requested_then_the_body_is_json() {
        let (app, _dir) = app();
        let (status, body) = get_json(app, "/api/project?sims=200&round=2").await;
        assert!(
            status == StatusCode::OK || status == StatusCode::BAD_REQUEST,
            "{status} {body}"
        );
        assert!(body.is_object());
        if status == StatusCode::OK {
            assert_eq!(body["round"], 2);
            assert_eq!(body["sims"], 200);
        } else {
            assert!(body["error"].is_string());
        }
    }

    #[tokio::test]
    async fn given_a_malformed_body_when_optimize_is_posted_then_400_with_an_error_field() {
        let (app, _dir) = app();
        let (status, body) = post_json(app, "/api/optimize", "{not json").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("bad request body"));
    }

    #[tokio::test]
    async fn given_an_unknown_chip_when_optimize_is_posted_then_400_names_the_chip() {
        let (app, _dir) = app();
        let (status, body) = post_json(
            app,
            "/api/optimize",
            r#"{"round":2,"sims":200,"chip":"bogus"}"#,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("bogus"));
    }

    #[tokio::test]
    async fn given_the_tiny_season_when_prices_are_requested_then_a_report_returns() {
        let (app, _dir) = app();
        let (status, body) = get_json(app, "/api/prices").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["report"].is_object());
        assert!(body["predictions"].is_array());
    }

    #[tokio::test]
    async fn given_the_tiny_season_when_a_backtest_is_requested_then_an_empty_report_returns() {
        let (app, _dir) = app();
        let (status, body) = get_json(app, "/api/backtest?sims=50").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["rounds"].is_array());
    }

    #[tokio::test]
    async fn given_a_ui_directory_when_a_client_route_is_requested_then_index_html_is_served() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<html>ui</html>").unwrap();
        let store = Arc::new(JsonSeasonRepository::new(dir.path().join("season.json")));
        let state = AppState {
            ctx: AnalysisContext::new(tiny_season(), ScoringRules::default()),
            sync: use_case(Arc::clone(&store)),
            store,
            season: 2026,
        };
        let app = router(Arc::new(state), Some(dir.path()));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/optimize")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&bytes[..], b"<html>ui</html>");
    }
}
