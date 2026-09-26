//! The Jolpica race-data gateway.
//!
//! Jolpica is the maintained successor of the Ergast API. It publishes the
//! calendar and every classification of a season as JSON. The API encodes
//! every number as a string, and it paginates by result row. One round can
//! span two pages; the client merges the rows of a round into one record.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use overcut_application::ports::{
    CalendarEntry, ClassifiedCar, GatewayError, QualifyingClassification, RaceClassification,
    RaceDataGateway,
};
use overcut_domain::season::Session;
use overcut_domain::shared::Tla;
use reqwest::StatusCode;
use serde::Deserialize;

/// The public API location.
const DEFAULT_BASE_URL: &str = "https://api.jolpi.ca/ergast/f1";

/// The page size of every request. Jolpica accepts at most 100.
const PAGE_SIZE: u32 = 100;

/// The gateway name in every error.
const GATEWAY: &str = "jolpica";

/// The date layout of the API.
const DATE_LAYOUT: &str = "%Y-%m-%d";

/// The layout of a date and a UTC time joined with one space.
const DATE_TIME_LAYOUT: &str = "%Y-%m-%d %H:%M:%SZ";

/// Reads the season calendar and every classification from the Jolpica
/// API.
#[derive(Debug, Clone)]
pub struct JolpicaClient {
    base_url: String,
    http: reqwest::Client,
}

impl JolpicaClient {
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

    /// The base URL of the API.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Fetches every page of one endpoint and merges the rows into one race
    /// per (season, round), in first-seen order.
    async fn get(&self, path: &str) -> Result<Vec<Race>, GatewayError> {
        let mut merged: Vec<Race> = Vec::new();
        let mut index: HashMap<(String, String), usize> = HashMap::new();
        let mut offset = 0;
        loop {
            let url = format!(
                "{}/{path}.json?limit={PAGE_SIZE}&offset={offset}",
                self.base_url
            );
            let MrData { total, race_table } = self.fetch_page(&url).await?.mr_data;
            let page_is_empty = race_table.races.is_empty();
            for race in race_table.races {
                let key = (race.season.clone(), race.round.clone());
                if let Some(&i) = index.get(&key) {
                    merged[i].results.extend(race.results);
                    merged[i].qualifying_results.extend(race.qualifying_results);
                    merged[i].sprint_results.extend(race.sprint_results);
                } else {
                    index.insert(key, merged.len());
                    merged.push(race);
                }
            }
            offset += PAGE_SIZE;
            if offset >= lenient_u32(&total) || page_is_empty {
                break;
            }
        }
        Ok(merged)
    }

    async fn fetch_page(&self, url: &str) -> Result<Envelope, GatewayError> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| GatewayError::new(GATEWAY, format!("get {url}: {e}")))?;
        let status = response.status();
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
        serde_json::from_slice(&body)
            .map_err(|e| GatewayError::new(GATEWAY, format!("parse {url}: {e}")))
    }
}

impl Default for JolpicaClient {
    fn default() -> Self {
        Self::new(DEFAULT_BASE_URL)
    }
}

#[async_trait]
impl RaceDataGateway for JolpicaClient {
    async fn calendar(&self, season: u16) -> Result<Vec<CalendarEntry>, GatewayError> {
        let races = self.get(&season.to_string()).await?;
        Ok(races.iter().map(calendar_entry).collect())
    }

    async fn qualifying(&self, season: u16) -> Result<Vec<QualifyingClassification>, GatewayError> {
        let races = self.get(&format!("{season}/qualifying")).await?;
        Ok(races.iter().map(qualifying_classification).collect())
    }

    async fn sprints(&self, season: u16) -> Result<Vec<RaceClassification>, GatewayError> {
        let races = self.get(&format!("{season}/sprint")).await?;
        Ok(races
            .iter()
            .map(|r| race_classification(r, &r.sprint_results))
            .collect())
    }

    async fn races(&self, season: u16) -> Result<Vec<RaceClassification>, GatewayError> {
        let races = self.get(&format!("{season}/results")).await?;
        Ok(races
            .iter()
            .map(|r| race_classification(r, &r.results))
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Mapping to the application records
// ---------------------------------------------------------------------------

fn calendar_entry(race: &Race) -> CalendarEntry {
    let mut sessions = Vec::new();
    let mut add = |session: Session, entry: Option<&ScheduleEntry>| {
        if let Some(start) = entry.and_then(|e| session_time(&e.date, &e.time)) {
            sessions.push((session, start));
        }
    };
    add(Session::FP1, race.first_practice.as_ref());
    add(Session::FP2, race.second_practice.as_ref());
    add(Session::FP3, race.third_practice.as_ref());
    add(Session::SprintQualifying, race.sprint_qualifying.as_ref());
    add(Session::Sprint, race.sprint.as_ref());
    add(Session::Qualifying, race.qualifying.as_ref());
    let race_start = ScheduleEntry {
        date: race.date.clone(),
        time: race.time.clone(),
    };
    add(Session::Race, Some(&race_start));

    CalendarEntry {
        round: lenient_u32(&race.round),
        name: race.name.clone(),
        circuit_id: race.circuit.circuit_id.clone(),
        date: NaiveDate::parse_from_str(&race.date, DATE_LAYOUT).ok(),
        has_sprint: race.sprint.is_some(),
        sessions,
    }
}

fn qualifying_classification(race: &Race) -> QualifyingClassification {
    let positions = race
        .qualifying_results
        .iter()
        .filter_map(|q| driver_code(&q.driver).map(|tla| (tla, lenient_u32(&q.position))))
        .collect();
    QualifyingClassification {
        round: lenient_u32(&race.round),
        positions,
    }
}

fn race_classification(race: &Race, rows: &[ResultRow]) -> RaceClassification {
    RaceClassification {
        round: lenient_u32(&race.round),
        cars: rows.iter().filter_map(classified_car).collect(),
    }
}

fn classified_car(row: &ResultRow) -> Option<ClassifiedCar> {
    let tla = driver_code(&row.driver)?;
    Some(ClassifiedCar {
        tla,
        grid: lenient_u32(&row.grid),
        position: lenient_u32(&row.position),
        classified: is_classified(&row.position_text),
        fastest_lap: row.fastest_lap.as_ref().is_some_and(|f| f.rank == "1"),
    })
}

/// Parses the driver code of a row. A row with an unusable code is skipped
/// and logged.
fn driver_code(driver: &Driver) -> Option<Tla> {
    match Tla::parse(&driver.code) {
        Ok(tla) => Some(tla),
        Err(e) => {
            tracing::warn!(driver = %driver.driver_id, error = %e, "skipping a row without a usable driver code");
            None
        }
    }
}

/// Reports whether a result row is a classified finish. A classified car
/// carries a numeric `positionText`. A retired, disqualified, excluded, or
/// withdrawn car carries a letter (R, D, E, W).
fn is_classified(position_text: &str) -> bool {
    position_text
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_digit)
}

/// Parses a calendar date and UTC time. A missing or malformed value is not
/// a session start.
fn session_time(date: &str, time: &str) -> Option<DateTime<Utc>> {
    if date.is_empty() || time.is_empty() {
        return None;
    }
    NaiveDateTime::parse_from_str(&format!("{date} {time}"), DATE_TIME_LAYOUT)
        .ok()
        .map(|t| t.and_utc())
}

/// Parses a numeric API string. The API encodes every number as a string.
/// An empty or malformed value parses as zero.
fn lenient_u32(s: &str) -> u32 {
    s.trim().parse().unwrap_or(0)
}

// ---------------------------------------------------------------------------
// The wire format
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Envelope {
    #[serde(rename = "MRData")]
    mr_data: MrData,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct MrData {
    total: String,
    #[serde(rename = "RaceTable")]
    race_table: RaceTable,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RaceTable {
    #[serde(rename = "Races")]
    races: Vec<Race>,
}

/// The schedule entry and the results of one round.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Race {
    season: String,
    round: String,
    #[serde(rename = "raceName")]
    name: String,
    date: String,
    time: String,
    #[serde(rename = "Circuit")]
    circuit: Circuit,
    #[serde(rename = "Results")]
    results: Vec<ResultRow>,
    #[serde(rename = "QualifyingResults")]
    qualifying_results: Vec<QualiRow>,
    #[serde(rename = "SprintResults")]
    sprint_results: Vec<ResultRow>,
    #[serde(rename = "FirstPractice")]
    first_practice: Option<ScheduleEntry>,
    #[serde(rename = "SecondPractice")]
    second_practice: Option<ScheduleEntry>,
    #[serde(rename = "ThirdPractice")]
    third_practice: Option<ScheduleEntry>,
    #[serde(rename = "Qualifying")]
    qualifying: Option<ScheduleEntry>,
    #[serde(rename = "Sprint")]
    sprint: Option<ScheduleEntry>,
    #[serde(rename = "SprintQualifying")]
    sprint_qualifying: Option<ScheduleEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Circuit {
    #[serde(rename = "circuitId")]
    circuit_id: String,
}

/// A session on the calendar. The time is in UTC and may be empty when the
/// source has no time yet.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ScheduleEntry {
    date: String,
    time: String,
}

/// One row of a race or sprint classification.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ResultRow {
    position: String,
    #[serde(rename = "positionText")]
    position_text: String,
    grid: String,
    #[serde(rename = "Driver")]
    driver: Driver,
    #[serde(rename = "FastestLap")]
    fastest_lap: Option<FastestLap>,
}

/// One row of a qualifying classification.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct QualiRow {
    position: String,
    #[serde(rename = "Driver")]
    driver: Driver,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Driver {
    #[serde(rename = "driverId")]
    driver_id: String,
    code: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FastestLap {
    rank: String,
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn tla(code: &str) -> Tla {
        Tla::parse(code).unwrap()
    }

    fn result_row(
        code: &str,
        position: &str,
        position_text: &str,
        grid: &str,
        fastest_rank: Option<&str>,
    ) -> serde_json::Value {
        let mut row = json!({
            "position": position,
            "positionText": position_text,
            "grid": grid,
            "status": "Finished",
            "Driver": {"driverId": code.to_lowercase(), "code": code, "givenName": "A", "familyName": "B"},
            "Constructor": {"constructorId": "team", "name": "Team"},
        });
        if let Some(rank) = fastest_rank {
            row["FastestLap"] = json!({"rank": rank});
        }
        row
    }

    fn envelope(total: &str, races: &[serde_json::Value]) -> serde_json::Value {
        json!({"MRData": {"total": total, "RaceTable": {"Races": races}}})
    }

    async fn mount(server: &MockServer, endpoint: &str, offset: &str, body: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(endpoint))
            .and(query_param("limit", "100"))
            .and(query_param("offset", offset))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn given_a_round_split_over_two_pages_when_races_are_fetched_then_the_rows_merge_into_one_round(
    ) {
        let server = MockServer::start().await;
        let page_one = envelope(
            "150",
            &[json!({
                "season": "2026", "round": "1", "raceName": "Australian Grand Prix",
                "date": "2026-03-08", "time": "04:00:00Z",
                "Circuit": {"circuitId": "albert_park"},
                "Results": [
                    result_row("VER", "1", "1", "2", Some("1")),
                    result_row("HAM", "20", "R", "3", Some("5")),
                ],
            })],
        );
        let page_two = envelope(
            "150",
            &[json!({
                "season": "2026", "round": "1", "raceName": "Australian Grand Prix",
                "date": "2026-03-08", "time": "04:00:00Z",
                "Circuit": {"circuitId": "albert_park"},
                "Results": [result_row("BOT", "16", "16", "0", None)],
            })],
        );
        mount(&server, "/2026/results.json", "0", page_one).await;
        mount(&server, "/2026/results.json", "100", page_two).await;
        let client = JolpicaClient::new(server.uri());

        let races = client.races(2026).await.unwrap();

        assert_eq!(races.len(), 1);
        let race = &races[0];
        assert_eq!(race.round, 1);
        assert_eq!(race.cars.len(), 3);
        assert_eq!(
            race.cars[0],
            ClassifiedCar {
                tla: tla("VER"),
                grid: 2,
                position: 1,
                classified: true,
                fastest_lap: true
            }
        );
        assert_eq!(
            race.cars[1],
            ClassifiedCar {
                tla: tla("HAM"),
                grid: 3,
                position: 20,
                classified: false,
                fastest_lap: false
            }
        );
        assert_eq!(
            race.cars[2],
            ClassifiedCar {
                tla: tla("BOT"),
                grid: 0,
                position: 16,
                classified: true,
                fastest_lap: false
            }
        );
    }

    #[tokio::test]
    async fn given_a_calendar_page_when_fetched_then_sessions_and_the_sprint_flag_are_mapped() {
        let server = MockServer::start().await;
        let body = envelope(
            "2",
            &[
                json!({
                    "season": "2026", "round": "1", "raceName": "Australian Grand Prix",
                    "date": "2026-03-08", "time": "04:00:00Z",
                    "Circuit": {"circuitId": "albert_park"},
                    "FirstPractice": {"date": "2026-03-06", "time": "01:30:00Z"},
                    "Qualifying": {"date": "2026-03-07", "time": "05:00:00Z"},
                }),
                json!({
                    "season": "2026", "round": "2", "raceName": "Chinese Grand Prix",
                    "date": "2026-03-15", "time": "07:00:00Z",
                    "Circuit": {"circuitId": "shanghai"},
                    "SprintQualifying": {"date": "2026-03-13", "time": "07:30:00Z"},
                    "Sprint": {"date": "2026-03-14", "time": "03:00:00Z"},
                    "Qualifying": {"date": "2026-03-14"},
                }),
            ],
        );
        mount(&server, "/2026.json", "0", body).await;
        let client = JolpicaClient::new(format!("{}/", server.uri()));

        let calendar = client.calendar(2026).await.unwrap();

        assert_eq!(calendar.len(), 2);
        let first = &calendar[0];
        assert_eq!(first.round, 1);
        assert_eq!(first.name, "Australian Grand Prix");
        assert_eq!(first.circuit_id, "albert_park");
        assert_eq!(first.date, NaiveDate::from_ymd_opt(2026, 3, 8));
        assert!(!first.has_sprint);
        assert_eq!(
            first.sessions,
            &[
                (
                    Session::FP1,
                    Utc.with_ymd_and_hms(2026, 3, 6, 1, 30, 0).unwrap()
                ),
                (
                    Session::Qualifying,
                    Utc.with_ymd_and_hms(2026, 3, 7, 5, 0, 0).unwrap()
                ),
                (
                    Session::Race,
                    Utc.with_ymd_and_hms(2026, 3, 8, 4, 0, 0).unwrap()
                ),
            ]
        );
        let second = &calendar[1];
        assert!(second.has_sprint);
        let labels: Vec<Session> = second.sessions.iter().map(|(s, _)| *s).collect();
        assert_eq!(
            labels,
            vec![Session::SprintQualifying, Session::Sprint, Session::Race]
        );
    }

    #[tokio::test]
    async fn given_a_qualifying_page_when_fetched_then_positions_are_keyed_by_code_and_bad_codes_are_skipped(
    ) {
        let server = MockServer::start().await;
        let body = envelope(
            "3",
            &[json!({
                "season": "2026", "round": "1", "raceName": "Australian Grand Prix",
                "date": "2026-03-08",
                "Circuit": {"circuitId": "albert_park"},
                "QualifyingResults": [
                    {"position": "1", "Driver": {"driverId": "russell", "code": "RUS"}},
                    {"position": "2", "Driver": {"driverId": "antonelli", "code": "ANT"}},
                    {"position": "3", "Driver": {"driverId": "nobody", "code": ""}},
                ],
            })],
        );
        mount(&server, "/2026/qualifying.json", "0", body).await;
        let client = JolpicaClient::new(server.uri());

        let quali = client.qualifying(2026).await.unwrap();

        assert_eq!(quali.len(), 1);
        assert_eq!(quali[0].round, 1);
        assert_eq!(quali[0].positions, vec![(tla("RUS"), 1), (tla("ANT"), 2)]);
    }

    #[tokio::test]
    async fn given_a_sprint_page_when_fetched_then_the_sprint_rows_are_read() {
        let server = MockServer::start().await;
        let body = envelope(
            "1",
            &[json!({
                "season": "2026", "round": "2", "raceName": "Chinese Grand Prix",
                "date": "2026-03-15",
                "Circuit": {"circuitId": "shanghai"},
                "SprintResults": [result_row("PIA", "1", "1", "1", None)],
            })],
        );
        mount(&server, "/2026/sprint.json", "0", body).await;
        let client = JolpicaClient::new(server.uri());

        let sprints = client.sprints(2026).await.unwrap();

        assert_eq!(sprints.len(), 1);
        assert_eq!(sprints[0].round, 2);
        assert_eq!(
            sprints[0].cars,
            vec![ClassifiedCar {
                tla: tla("PIA"),
                grid: 1,
                position: 1,
                classified: true,
                fastest_lap: false
            }]
        );
    }

    #[tokio::test]
    async fn given_a_server_error_when_fetched_then_a_jolpica_gateway_error_is_returned() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/2026/results.json"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let client = JolpicaClient::new(server.uri());

        let err = client.races(2026).await.unwrap_err();

        assert_eq!(err.gateway, "jolpica");
        assert!(err.message.contains("status 503"), "{}", err.message);
    }

    #[test]
    fn given_the_position_text_encoding_when_classified_then_digits_pass_and_letters_fail() {
        assert!(is_classified("1"));
        assert!(is_classified("16"));
        assert!(!is_classified("R"));
        assert!(!is_classified("D"));
        assert!(!is_classified("W"));
        assert!(!is_classified("E"));
        assert!(!is_classified(""));
    }

    #[test]
    fn given_malformed_numbers_when_parsed_then_they_read_as_zero() {
        assert_eq!(lenient_u32(""), 0);
        assert_eq!(lenient_u32("abc"), 0);
        assert_eq!(lenient_u32("-3"), 0);
        assert_eq!(lenient_u32("12"), 12);
    }

    #[test]
    fn given_a_missing_time_when_a_session_is_parsed_then_there_is_no_start() {
        assert_eq!(session_time("2026-03-08", ""), None);
        assert_eq!(session_time("", "04:00:00Z"), None);
        assert_eq!(session_time("2026-03-08", "04:00"), None);
        assert_eq!(
            session_time("2026-03-08", "04:00:00Z"),
            Some(Utc.with_ymd_and_hms(2026, 3, 8, 4, 0, 0).unwrap())
        );
    }
}
