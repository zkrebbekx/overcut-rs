//! The JSON codec of the `Season` aggregate.
//!
//! The JSON format is the format of the original Go toolkit, so a file
//! that the Go toolkit wrote decodes without change. The domain types
//! carry no `serde` derives; this crate holds private wire records and
//! maps them to and from the domain.
//!
//! The crate has no I/O. The file store in the infrastructure crate and
//! the WebAssembly build both use it.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use overcut_domain::season::{
    Asset, ComponentStats, GamedaySnapshot, RaceRow, Round, Season, Session,
};
use overcut_domain::shared::{AssetId, AssetKind, Gameday, RoundNumber, TeamId, Tla};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The date layout of a round in the file.
const DATE_LAYOUT: &str = "%Y-%m-%d";

/// A failure to decode or encode a season.
#[derive(Debug, Error)]
pub enum CodecError {
    /// The JSON does not parse.
    #[error("cannot parse season JSON: {0}")]
    Parse(#[from] serde_json::Error),
    /// The JSON parses but holds a value the domain rejects.
    #[error("invalid season: {0}")]
    Invalid(String),
}

/// Decodes a season from JSON text.
pub fn decode(json: &str) -> Result<Season, CodecError> {
    let file: SeasonFile = serde_json::from_str(json)?;
    season_from_file(file)
}

/// Decodes a season from JSON bytes.
pub fn decode_bytes(json: &[u8]) -> Result<Season, CodecError> {
    let file: SeasonFile = serde_json::from_slice(json)?;
    season_from_file(file)
}

/// Encodes a season as pretty-printed JSON text.
pub fn encode(season: &Season) -> Result<String, CodecError> {
    Ok(serde_json::to_string_pretty(&season_to_file(season))?)
}

// ---------------------------------------------------------------------------
// File → domain
// ---------------------------------------------------------------------------

fn season_from_file(file: SeasonFile) -> Result<Season, CodecError> {
    let rounds = file
        .rounds
        .into_iter()
        .map(round_from_file)
        .collect::<Result<Vec<_>, _>>()?;
    let assets = file
        .assets
        .into_iter()
        .map(asset_from_file)
        .collect::<Result<Vec<_>, _>>()?;
    Season::new(file.season, file.synced_at, rounds, assets)
        .map_err(|e| CodecError::Invalid(e.to_string()))
}

fn round_from_file(r: RoundFile) -> Result<Round, CodecError> {
    let number = RoundNumber::new(r.round)
        .map_err(|e| CodecError::Invalid(format!("round {}: {e}", r.round)))?;
    let context = |what: &str| format!("round {}: {what}", r.round);
    let mut round = Round::new(number, r.name);
    round.circuit_id = r.circuit_id;
    round.date = parse_date(&r.date).map_err(|e| CodecError::Invalid(context(&e)))?;
    round.has_sprint = r.has_sprint;
    round.has_results = r.has_results;
    round.sessions = r
        .sessions
        .into_iter()
        .filter_map(|(label, start)| Session::parse(&label).map(|s| (s, start)))
        .collect();
    round.points_seen_at = r.points_seen_at.filter(|t| *t != go_zero_time());
    round.quali = tla_map(r.quali).map_err(|e| CodecError::Invalid(context(&e)))?;
    round.grid = tla_map(r.grid).map_err(|e| CodecError::Invalid(context(&e)))?;
    round.race = tla_map(r.race)
        .map_err(|e| CodecError::Invalid(context(&e)))?
        .into_iter()
        .map(|(t, row)| (t, race_row_from_file(&row)))
        .collect();
    round.sprint = tla_map(r.sprint)
        .map_err(|e| CodecError::Invalid(context(&e)))?
        .into_iter()
        .map(|(t, row)| (t, race_row_from_file(&row)))
        .collect();
    Ok(round)
}

/// Re-keys a map by parsed driver code.
fn tla_map<V>(map: BTreeMap<String, V>) -> Result<BTreeMap<Tla, V>, String> {
    map.into_iter()
        .map(|(code, v)| Tla::parse(&code).map(|t| (t, v)).map_err(|e| e.to_string()))
        .collect()
}

fn race_row_from_file(row: &RaceRowFile) -> RaceRow {
    RaceRow {
        grid: row.grid,
        pos: row.pos,
        dnf: row.dnf,
        fastest_lap: row.fastest_lap,
    }
}

fn asset_from_file(a: AssetFile) -> Result<Asset, CodecError> {
    let context = |what: String| CodecError::Invalid(format!("asset {:?}: {what}", a.id));
    let id = AssetId::new(a.id.clone()).map_err(|e| context(e.to_string()))?;
    let kind = match a.kind.as_str() {
        "driver" => AssetKind::Driver,
        "constructor" => AssetKind::Constructor,
        other => return Err(context(format!("unknown asset kind {other:?}"))),
    };
    let tla = if a.tla.is_empty() {
        None
    } else {
        Some(Tla::parse(&a.tla).map_err(|e| context(e.to_string()))?)
    };
    let team_id = TeamId::new(a.team_id).map_err(|e| context(e.to_string()))?;
    let history = a
        .history
        .iter()
        .map(|h| snapshot_from_file(h).map_err(&context))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Asset {
        id,
        kind,
        name: a.name,
        tla,
        team_id,
        team_name: a.team_name,
        history,
    })
}

fn snapshot_from_file(h: &HistoryFile) -> Result<GamedaySnapshot, String> {
    let gameday = Gameday::new(h.gameday).map_err(|e| e.to_string())?;
    Ok(GamedaySnapshot {
        gameday,
        active: h.active,
        price: h.price,
        old_price: h.old_price,
        ownership: h.ownership,
        points: h.points,
        quali_pts: h.quali_pts,
        sprint_pts: h.sprint_pts,
        race_pts: h.race_pts,
        stats: ComponentStats {
            fastest_lap_pts: h.stats.fastest_lap_pts,
            dotd_pts: h.stats.dotd_pts,
            overtaking_pts: h.stats.overtaking_pts,
            q3_finishes_pts: h.stats.q3_finishes_pts,
            position_pts: h.stats.total_position_pts,
            pos_gained_lost: h.stats.total_position_gained_lost,
            dnf_pts: h.stats.total_dnf_dq_pts,
            value_for_money: h.stats.value_for_money,
            top10_race_pts: h.stats.top10_race_position_pts,
            top8_sprint_pts: h.stats.top8_sprint_position_pts,
        },
    })
}

/// Parses a round date. An empty string is no date.
fn parse_date(raw: &str) -> Result<Option<NaiveDate>, String> {
    if raw.is_empty() {
        return Ok(None);
    }
    NaiveDate::parse_from_str(raw, DATE_LAYOUT)
        .map(Some)
        .map_err(|e| format!("invalid date {raw:?}: {e}"))
}

/// The Go zero time. The Go toolkit writes it for a round whose points were
/// never seen. It means "no value".
fn go_zero_time() -> DateTime<Utc> {
    NaiveDate::from_ymd_opt(1, 1, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map_or_else(DateTime::<Utc>::default, |t| t.and_utc())
}

// ---------------------------------------------------------------------------
// Domain → file
// ---------------------------------------------------------------------------

fn season_to_file(season: &Season) -> SeasonFile {
    SeasonFile {
        season: season.year(),
        synced_at: season.synced_at(),
        rounds: season.rounds().iter().map(round_to_file).collect(),
        assets: season.assets().iter().map(asset_to_file).collect(),
    }
}

fn round_to_file(r: &Round) -> RoundFile {
    RoundFile {
        round: r.number.get(),
        name: r.name.clone(),
        circuit_id: r.circuit_id.clone(),
        date: r
            .date
            .map(|d| d.format(DATE_LAYOUT).to_string())
            .unwrap_or_default(),
        has_sprint: r.has_sprint,
        has_results: r.has_results,
        sessions: r
            .sessions
            .iter()
            .map(|(s, t)| (s.as_str().to_string(), *t))
            .collect(),
        points_seen_at: r.points_seen_at,
        quali: r.quali.iter().map(|(t, p)| (t.to_string(), *p)).collect(),
        grid: r.grid.iter().map(|(t, p)| (t.to_string(), *p)).collect(),
        race: r
            .race
            .iter()
            .map(|(t, row)| (t.to_string(), race_row_to_file(*row)))
            .collect(),
        sprint: r
            .sprint
            .iter()
            .map(|(t, row)| (t.to_string(), race_row_to_file(*row)))
            .collect(),
    }
}

fn race_row_to_file(row: RaceRow) -> RaceRowFile {
    RaceRowFile {
        grid: row.grid,
        pos: row.pos,
        dnf: row.dnf,
        fastest_lap: row.fastest_lap,
    }
}

fn asset_to_file(a: &Asset) -> AssetFile {
    AssetFile {
        id: a.id.to_string(),
        kind: a.kind.as_str().to_string(),
        name: a.name.clone(),
        tla: a.tla.as_ref().map(ToString::to_string).unwrap_or_default(),
        team_id: a.team_id.to_string(),
        team_name: a.team_name.clone(),
        history: a.history.iter().map(snapshot_to_file).collect(),
    }
}

fn snapshot_to_file(h: &GamedaySnapshot) -> HistoryFile {
    HistoryFile {
        gameday: h.gameday.get(),
        active: h.active,
        price: h.price,
        old_price: h.old_price,
        ownership: h.ownership,
        points: h.points,
        quali_pts: h.quali_pts,
        sprint_pts: h.sprint_pts,
        race_pts: h.race_pts,
        stats: StatsFile {
            fastest_lap_pts: h.stats.fastest_lap_pts,
            dotd_pts: h.stats.dotd_pts,
            overtaking_pts: h.stats.overtaking_pts,
            q3_finishes_pts: h.stats.q3_finishes_pts,
            total_position_pts: h.stats.position_pts,
            total_position_gained_lost: h.stats.pos_gained_lost,
            total_dnf_dq_pts: h.stats.dnf_pts,
            value_for_money: h.stats.value_for_money,
            top10_race_position_pts: h.stats.top10_race_pts,
            top8_sprint_position_pts: h.stats.top8_sprint_pts,
        },
    }
}

// ---------------------------------------------------------------------------
// The file format (the Go `dataset` package)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct SeasonFile {
    season: u16,
    synced_at: DateTime<Utc>,
    #[serde(default)]
    rounds: Vec<RoundFile>,
    #[serde(default)]
    assets: Vec<AssetFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RoundFile {
    round: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    circuit_id: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    has_sprint: bool,
    #[serde(default)]
    has_results: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    sessions: BTreeMap<String, DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    points_seen_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    quali: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    grid: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    race: BTreeMap<String, RaceRowFile>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    sprint: BTreeMap<String, RaceRowFile>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct RaceRowFile {
    grid: u32,
    pos: u32,
    dnf: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    fastest_lap: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct AssetFile {
    id: String,
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    tla: String,
    #[serde(default)]
    team_id: String,
    #[serde(default)]
    team_name: String,
    #[serde(default)]
    history: Vec<HistoryFile>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct HistoryFile {
    gameday: u32,
    active: bool,
    price: f64,
    old_price: f64,
    ownership: f64,
    points: f64,
    quali_pts: f64,
    sprint_pts: f64,
    race_pts: f64,
    stats: StatsFile,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct StatsFile {
    fastest_lap_pts: f64,
    dotd_pts: f64,
    overtaking_pts: f64,
    q3_finishes_pts: f64,
    total_position_pts: f64,
    total_position_gained_lost: f64,
    total_dnf_dq_pts: f64,
    value_for_money: f64,
    top10_race_position_pts: f64,
    top8_sprint_position_pts: f64,
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    const FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/season2026.json"
    );

    mod given_the_go_fixture {
        use super::*;

        #[test]
        fn when_decoded_then_it_has_the_known_shape() {
            let season = decode(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
            assert_eq!(season.year(), 2026);
            assert_eq!(season.rounds().len(), 23);
            assert_eq!(season.assets().len(), 35);
            assert!(season.rounds()[1].has_sprint && !season.rounds()[1].sprint.is_empty());
        }

        #[test]
        fn when_encoded_and_decoded_again_then_the_season_is_unchanged() {
            let season = decode(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
            let again = decode(&encode(&season).unwrap()).unwrap();
            assert_eq!(again, season);
        }
    }

    mod given_go_encoded_values {
        use super::*;

        #[test]
        fn when_the_zero_time_is_decoded_then_points_seen_at_is_none() {
            let json = r#"{"season":2026,"synced_at":"2026-01-01T00:00:00Z","rounds":[{"round":1,"points_seen_at":"0001-01-01T00:00:00Z"}]}"#;
            let season = decode(json).unwrap();
            assert_eq!(season.rounds()[0].points_seen_at, None);
        }

        #[test]
        fn when_an_unknown_asset_kind_is_decoded_then_it_is_rejected() {
            let json = r#"{"season":2026,"synced_at":"2026-01-01T00:00:00Z","assets":[{"id":"1","kind":"engine"}]}"#;
            assert!(matches!(decode(json), Err(CodecError::Invalid(_))));
        }

        #[test]
        fn when_a_round_date_is_decoded_then_it_is_a_calendar_date() {
            let json = r#"{"season":2026,"synced_at":"2026-01-01T00:00:00Z","rounds":[{"round":3,"date":"2026-03-08"}]}"#;
            let season = decode(json).unwrap();
            assert_eq!(season.rounds()[0].date, NaiveDate::from_ymd_opt(2026, 3, 8));
            let _ = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0);
        }
    }
}
