//! The F1 Fantasy feed gateway.
//!
//! The game publishes one JSON document per gameday at
//! `https://fantasy.formula1.com/feeds/drivers/<gameday>_en.json`. Each
//! document lists every driver and constructor with the current price, the
//! previous price, the ownership share, and the official points per session.
//!
//! The feed has two quirks that the adapter resolves:
//!
//! - Several numbers arrive as strings. A malformed string reads as zero.
//! - The feed labels the sprint race "Sprint Qualifying". Sprint qualifying
//!   itself scores nothing, so both labels add into the sprint points.

use std::time::Duration;

use async_trait::async_trait;
use overcut_application::ports::{FantasyFeedGateway, FeedGameday, FeedPlayer, GatewayError};
use overcut_domain::season::ComponentStats;
use overcut_domain::shared::AssetKind;
use reqwest::StatusCode;
use serde::Deserialize;

/// The public feed location.
const DEFAULT_BASE_URL: &str = "https://fantasy.formula1.com/feeds/drivers";

/// The gateway name in every error.
const GATEWAY: &str = "fantasy_feed";

/// The `Skill` value of a driver row. Every other value is a constructor.
const SKILL_DRIVER: i64 = 1;

/// Reads gameday documents from the F1 Fantasy feed.
#[derive(Debug, Clone)]
pub struct FantasyFeedClient {
    base_url: String,
    http: reqwest::Client,
}

impl FantasyFeedClient {
    /// Builds a client against `base_url`. The client removes a trailing
    /// slash. Every request times out after 30 seconds.
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let http = reqwest::Client::builder()
            .user_agent(crate::USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { base_url, http }
    }

    /// The base URL of the feed.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl Default for FantasyFeedClient {
    fn default() -> Self {
        Self::new(DEFAULT_BASE_URL)
    }
}

#[async_trait]
impl FantasyFeedGateway for FantasyFeedClient {
    async fn gameday(&self, gameday: u32) -> Result<Option<FeedGameday>, GatewayError> {
        let url = format!("{}/{gameday}_en.json", self.base_url);
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| GatewayError::new(GATEWAY, format!("get {url}: {e}")))?;
        let status = response.status();
        if status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status != StatusCode::OK {
            return Err(GatewayError::new(
                GATEWAY,
                format!("get {url}: status {}", status.as_u16()),
            ));
        }
        let body = response
            .bytes()
            .await
            .map_err(|e| GatewayError::new(GATEWAY, format!("read {url}: {e}")))?;
        let envelope: Envelope = serde_json::from_slice(&body)
            .map_err(|e| GatewayError::new(GATEWAY, format!("parse {url}: {e}")))?;
        let players = envelope.data.value.iter().map(feed_player).collect();
        Ok(Some(FeedGameday { gameday, players }))
    }
}

// ---------------------------------------------------------------------------
// Mapping to the application records
// ---------------------------------------------------------------------------

fn feed_player(p: &Player) -> FeedPlayer {
    let mut quali_pts = 0.0;
    let mut sprint_pts = 0.0;
    let mut race_pts = 0.0;
    for session in &p.sessions {
        match session.session_type.as_str() {
            "Qualifying" => quali_pts = session.points,
            // The feed labels the sprint race "Sprint Qualifying". Sprint
            // qualifying scores nothing, so both labels add into the sprint.
            "Sprint Qualifying" | "Sprint" => sprint_pts += session.points,
            "Race" => race_pts = session.points,
            _ => {}
        }
    }
    let kind = if p.skill == SKILL_DRIVER {
        AssetKind::Driver
    } else {
        AssetKind::Constructor
    };
    FeedPlayer {
        id: p.id.clone(),
        kind,
        name: p.full_name.clone(),
        tla: Some(p.driver_tla.clone()).filter(|t| !t.is_empty()),
        team_id: p.team_id.clone(),
        team_name: p.team_name.clone(),
        active: p.is_active == "1",
        price: p.value,
        old_price: p.old_value,
        ownership: lenient_f64(&p.selected),
        points: lenient_f64(&p.gameday_pts),
        quali_pts,
        sprint_pts,
        race_pts,
        stats: component_stats(&p.stats),
    }
}

fn component_stats(s: &AdditionalStats) -> ComponentStats {
    ComponentStats {
        fastest_lap_pts: s.fastest_lap_pts,
        dotd_pts: s.dotd_pts,
        overtaking_pts: s.overtaking_pts,
        q3_finishes_pts: s.q3_finishes_pts,
        position_pts: s.total_position_pts,
        pos_gained_lost: s.total_position_gained_lost,
        dnf_pts: s.total_dnf_dq_pts,
        value_for_money: s.value_for_money,
        top10_race_pts: s.top10_race_position_pts,
        top8_sprint_pts: s.top8_sprint_position_pts,
    }
}

/// Parses a numeric feed string. An empty or malformed value parses as zero.
fn lenient_f64(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}

/// Deserialises a feed number that may be `null`. The feed publishes
/// `null` for a session that has not scored yet; Go's decoder reads it as
/// zero, and so does this adapter.
fn null_f64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// The wire format
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Envelope {
    #[serde(rename = "Data")]
    data: Data,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Data {
    #[serde(rename = "Value")]
    value: Vec<Player>,
}

/// One asset (driver or constructor) in one gameday document. The field
/// names carry the feed's own casing, including `FUllName`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Player {
    #[serde(rename = "PlayerId")]
    id: String,
    #[serde(rename = "Skill")]
    skill: i64,
    #[serde(rename = "Value", deserialize_with = "null_f64")]
    value: f64,
    #[serde(rename = "OldPlayerValue", deserialize_with = "null_f64")]
    old_value: f64,
    #[serde(rename = "FUllName")]
    full_name: String,
    #[serde(rename = "TeamName")]
    team_name: String,
    #[serde(rename = "TeamId")]
    team_id: String,
    #[serde(rename = "IsActive")]
    is_active: String,
    #[serde(rename = "DriverTLA")]
    driver_tla: String,
    #[serde(rename = "GamedayPoints")]
    gameday_pts: String,
    #[serde(rename = "SelectedPercentage")]
    selected: String,
    #[serde(rename = "SessionWisePoints")]
    sessions: Vec<SessionPoints>,
    #[serde(rename = "AdditionalStats")]
    stats: AdditionalStats,
}

/// The official points for one session of one gameday.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SessionPoints {
    #[serde(rename = "sessiontype")]
    session_type: String,
    #[serde(deserialize_with = "null_f64")]
    points: f64,
}

/// The season-to-date component breakdown for one asset.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AdditionalStats {
    #[serde(deserialize_with = "null_f64")]
    fastest_lap_pts: f64,
    #[serde(deserialize_with = "null_f64")]
    dotd_pts: f64,
    #[serde(deserialize_with = "null_f64")]
    overtaking_pts: f64,
    #[serde(deserialize_with = "null_f64")]
    q3_finishes_pts: f64,
    #[serde(deserialize_with = "null_f64")]
    total_position_pts: f64,
    #[serde(deserialize_with = "null_f64")]
    total_position_gained_lost: f64,
    #[serde(deserialize_with = "null_f64")]
    total_dnf_dq_pts: f64,
    #[serde(deserialize_with = "null_f64")]
    value_for_money: f64,
    #[serde(deserialize_with = "null_f64")]
    top10_race_position_pts: f64,
    #[serde(deserialize_with = "null_f64")]
    top8_sprint_position_pts: f64,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn document() -> serde_json::Value {
        json!({"Data": {"Value": [
            {
                "PlayerId": "18", "Skill": 1, "PositionName": "DRIVER",
                "Value": 12.5, "OldPlayerValue": 12.0,
                "FUllName": "Pierre Gasly", "DisplayName": "P. Gasly",
                "TeamName": "Alpine", "TeamId": "23", "IsActive": "1", "DriverTLA": "GAS",
                "OverallPpints": "31", "GamedayPoints": "20", "SelectedPercentage": "13.4",
                "SessionWisePoints": [
                    {"sessionnumber": 1, "sessiontype": "Sprint Qualifying", "points": 3.0},
                    {"sessionnumber": 2, "sessiontype": "Sprint", "points": 4.0},
                    {"sessionnumber": 3, "sessiontype": "Qualifying", "points": 5.0},
                    {"sessionnumber": 4, "sessiontype": "Race", "points": 8.0},
                    {"sessionnumber": 5, "sessiontype": "Unknown", "points": 99.0}
                ],
                "AdditionalStats": {
                    "fastest_lap_pts": 10, "dotd_pts": 0, "overtaking_pts": 6, "q3_finishes_pts": 1,
                    "total_position_pts": 2, "total_position_gained_lost": 4, "total_dnf_dq_pts": -20,
                    "value_for_money": 0.92, "top10_race_position_pts": 1, "top8_sprint_position_pts": 3
                }
            },
            {
                "PlayerId": "28", "Skill": 2,
                "Value": 30.0, "OldPlayerValue": 30.0,
                "FUllName": "Mercedes", "TeamName": "", "TeamId": "28", "IsActive": "0", "DriverTLA": "",
                "GamedayPoints": "not-a-number", "SelectedPercentage": ""
            }
        ]}})
    }

    async fn server_with(status: u16, body: Option<serde_json::Value>) -> MockServer {
        let server = MockServer::start().await;
        let mut template = ResponseTemplate::new(status);
        if let Some(body) = body {
            template = template.set_body_json(body);
        }
        Mock::given(method("GET"))
            .and(path("/3_en.json"))
            .respond_with(template)
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn given_a_published_gameday_when_fetched_then_every_player_and_session_is_mapped() {
        let server = server_with(200, Some(document())).await;
        let client = FantasyFeedClient::new(server.uri());

        let gameday = client.gameday(3).await.unwrap().unwrap();

        assert_eq!(gameday.gameday, 3);
        assert_eq!(gameday.players.len(), 2);
        let gasly = &gameday.players[0];
        assert_eq!(gasly.id, "18");
        assert_eq!(gasly.kind, AssetKind::Driver);
        assert_eq!(gasly.name, "Pierre Gasly");
        assert_eq!(gasly.tla.as_deref(), Some("GAS"));
        assert_eq!(gasly.team_id, "23");
        assert_eq!(gasly.team_name, "Alpine");
        assert!(gasly.active);
        assert_eq!(gasly.price, 12.5);
        assert_eq!(gasly.old_price, 12.0);
        assert_eq!(gasly.ownership, 13.4);
        assert_eq!(gasly.points, 20.0);
        assert_eq!(gasly.quali_pts, 5.0);
        assert_eq!(
            gasly.sprint_pts, 7.0,
            "Sprint Qualifying and Sprint add into the sprint leg"
        );
        assert_eq!(gasly.race_pts, 8.0);
        assert_eq!(
            gasly.stats,
            ComponentStats {
                fastest_lap_pts: 10.0,
                dotd_pts: 0.0,
                overtaking_pts: 6.0,
                q3_finishes_pts: 1.0,
                position_pts: 2.0,
                pos_gained_lost: 4.0,
                dnf_pts: -20.0,
                value_for_money: 0.92,
                top10_race_pts: 1.0,
                top8_sprint_pts: 3.0,
            }
        );
    }

    #[tokio::test]
    async fn given_a_constructor_row_with_missing_fields_when_fetched_then_defaults_apply() {
        let server = server_with(200, Some(document())).await;
        let client = FantasyFeedClient::new(server.uri());

        let gameday = client.gameday(3).await.unwrap().unwrap();

        let mercedes = &gameday.players[1];
        assert_eq!(mercedes.kind, AssetKind::Constructor);
        assert_eq!(mercedes.tla, None);
        assert!(!mercedes.active);
        assert_eq!(mercedes.points, 0.0);
        assert_eq!(mercedes.ownership, 0.0);
        assert_eq!(mercedes.quali_pts, 0.0);
        assert_eq!(mercedes.sprint_pts, 0.0);
        assert_eq!(mercedes.race_pts, 0.0);
        assert_eq!(mercedes.stats, ComponentStats::default());
    }

    #[tokio::test]
    async fn given_a_not_found_response_when_fetched_then_the_gameday_is_not_published() {
        let server = server_with(404, None).await;
        let client = FantasyFeedClient::new(server.uri());

        assert_eq!(client.gameday(3).await.unwrap(), None);
    }

    #[tokio::test]
    async fn given_a_forbidden_response_when_fetched_then_the_gameday_is_not_published() {
        let server = server_with(403, None).await;
        let client = FantasyFeedClient::new(server.uri());

        assert_eq!(client.gameday(3).await.unwrap(), None);
    }

    #[tokio::test]
    async fn given_a_server_error_when_fetched_then_a_gateway_error_is_returned() {
        let server = server_with(500, None).await;
        let client = FantasyFeedClient::new(server.uri());

        let err = client.gameday(3).await.unwrap_err();

        assert_eq!(err.gateway, "fantasy_feed");
        assert!(err.message.contains("status 500"), "{}", err.message);
    }

    #[tokio::test]
    async fn given_an_empty_document_when_fetched_then_the_gameday_has_no_players() {
        let server = server_with(200, Some(json!({}))).await;
        let client = FantasyFeedClient::new(server.uri());

        let gameday = client.gameday(3).await.unwrap().unwrap();

        assert!(gameday.players.is_empty());
    }
    #[tokio::test]
    async fn given_a_null_session_score_when_the_gameday_is_fetched_then_it_parses_as_zero() {
        let server = MockServer::start().await;
        let body = json!({"Data": {"Value": [{
            "PlayerId": "1", "Skill": 1, "Value": 10.0, "OldPlayerValue": null, "FUllName": "A B",
            "TeamName": "T", "TeamId": "t", "IsActive": "1", "DriverTLA": "ABC", "GamedayPoints": "5",
            "SelectedPercentage": "1",
            "SessionWisePoints": [{"sessiontype": "Race", "points": null}],
            "AdditionalStats": {"dotd_pts": null}
        }]}});
        Mock::given(method("GET"))
            .and(path("/3_en.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let doc = FantasyFeedClient::new(server.uri())
            .gameday(3)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(doc.players[0].race_pts, 0.0);
        assert_eq!(doc.players[0].old_price, 0.0);
        assert_eq!(doc.players[0].stats.dotd_pts, 0.0);
    }
}
