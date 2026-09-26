//! The JSON file store of the `Season` aggregate.
//!
//! The store keeps the whole season in one JSON file. The file format is
//! the format of the original Go toolkit, so a file that the Go toolkit
//! wrote loads without change. The domain types carry no `serde` derives;
//! this module holds private file records and maps them to and from the
//! domain.
//!
//! A save writes to `<path>.tmp` and then renames the file into place, so a
//! reader never sees a partial file.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use overcut_domain::season::{
    Asset, ComponentStats, GamedaySnapshot, RaceRow, RepositoryError, Round, Season,
    SeasonRepository, Session,
};
use overcut_domain::shared::{AssetId, AssetKind, Gameday, RoundNumber, TeamId, Tla};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The date layout of a round in the file.
const DATE_LAYOUT: &str = "%Y-%m-%d";

/// Stores the season in one JSON file.
#[derive(Debug, Clone)]
pub struct JsonSeasonRepository {
    path: PathBuf,
}

impl JsonSeasonRepository {
    /// Builds a store that reads and writes `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn temp_path(&self) -> PathBuf {
        let mut name: OsString = self.path.clone().into_os_string();
        name.push(".tmp");
        PathBuf::from(name)
    }

    fn read_error(&self, source: io::Error) -> RepositoryError {
        RepositoryError::new(StoreError::Read {
            path: self.path.clone(),
            source,
        })
    }

    fn write_error(&self, source: io::Error) -> RepositoryError {
        RepositoryError::new(StoreError::Write {
            path: self.path.clone(),
            source,
        })
    }
}

impl SeasonRepository for JsonSeasonRepository {
    fn load(&self) -> Result<Option<Season>, RepositoryError> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(self.read_error(e)),
        };
        let file: SeasonFile = serde_json::from_slice(&bytes).map_err(|source| {
            RepositoryError::new(StoreError::Parse {
                path: self.path.clone(),
                source,
            })
        })?;
        season_from_file(file).map(Some)
    }

    fn save(&self, season: &Season) -> Result<(), RepositoryError> {
        if let Some(parent) = self.path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| self.write_error(e))?;
        }
        let file = season_to_file(season);
        let json = serde_json::to_string_pretty(&file).map_err(|source| {
            RepositoryError::new(StoreError::Encode {
                path: self.path.clone(),
                source,
            })
        })?;
        let tmp = self.temp_path();
        std::fs::write(&tmp, json).map_err(|e| self.write_error(e))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| self.write_error(e))
    }
}

/// A failure of the file store, with the path for context.
#[derive(Debug, Error)]
enum StoreError {
    #[error("cannot read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("cannot write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error("cannot parse {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("cannot encode {}: {source}", path.display())]
    Encode {
        path: PathBuf,
        source: serde_json::Error,
    },
}

// ---------------------------------------------------------------------------
// File → domain
// ---------------------------------------------------------------------------

fn season_from_file(file: SeasonFile) -> Result<Season, RepositoryError> {
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
    Season::new(file.season, file.synced_at, rounds, assets).map_err(RepositoryError::new)
}

fn round_from_file(r: RoundFile) -> Result<Round, RepositoryError> {
    let number = RoundNumber::new(r.round)
        .map_err(|e| RepositoryError::message(format!("round {}: {e}", r.round)))?;
    let context = |what: &str| format!("round {}: {what}", r.round);
    let mut round = Round::new(number, r.name);
    round.circuit_id = r.circuit_id;
    round.date = parse_date(&r.date).map_err(|e| RepositoryError::message(context(&e)))?;
    round.has_sprint = r.has_sprint;
    round.has_results = r.has_results;
    round.sessions = r
        .sessions
        .into_iter()
        .filter_map(|(label, start)| Session::parse(&label).map(|s| (s, start)))
        .collect();
    round.points_seen_at = r.points_seen_at.filter(|t| *t != go_zero_time());
    round.quali = tla_map(r.quali).map_err(|e| RepositoryError::message(context(&e)))?;
    round.grid = tla_map(r.grid).map_err(|e| RepositoryError::message(context(&e)))?;
    round.race = tla_map(r.race)
        .map_err(|e| RepositoryError::message(context(&e)))?
        .into_iter()
        .map(|(t, row)| (t, race_row_from_file(&row)))
        .collect();
    round.sprint = tla_map(r.sprint)
        .map_err(|e| RepositoryError::message(context(&e)))?
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

fn asset_from_file(a: AssetFile) -> Result<Asset, RepositoryError> {
    let context = |what: String| RepositoryError::message(format!("asset {:?}: {what}", a.id));
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

    fn tla(code: &str) -> Tla {
        Tla::parse(code).unwrap()
    }

    fn sample_season() -> Season {
        let mut r1 = Round::new(RoundNumber::new(1).unwrap(), "Australian Grand Prix");
        r1.circuit_id = "albert_park".into();
        r1.date = NaiveDate::from_ymd_opt(2026, 3, 8);
        r1.has_results = true;
        r1.sessions.insert(
            Session::Qualifying,
            Utc.with_ymd_and_hms(2026, 3, 7, 5, 0, 0).unwrap(),
        );
        r1.sessions.insert(
            Session::Race,
            Utc.with_ymd_and_hms(2026, 3, 8, 4, 0, 0).unwrap(),
        );
        r1.points_seen_at = Some(Utc.with_ymd_and_hms(2026, 3, 8, 9, 30, 15).unwrap());
        r1.quali.insert(tla("VER"), 1);
        r1.quali.insert(tla("HAM"), 2);
        r1.race.insert(
            tla("VER"),
            RaceRow {
                grid: 1,
                pos: 1,
                dnf: false,
                fastest_lap: true,
            },
        );
        r1.race.insert(
            tla("HAM"),
            RaceRow {
                grid: 2,
                pos: 20,
                dnf: true,
                fastest_lap: false,
            },
        );

        let mut r2 = Round::new(RoundNumber::new(2).unwrap(), "Chinese Grand Prix");
        r2.has_sprint = true;
        r2.grid.insert(tla("VER"), 3);
        r2.sprint.insert(
            tla("VER"),
            RaceRow {
                grid: 1,
                pos: 2,
                dnf: false,
                fastest_lap: false,
            },
        );

        let driver = Asset {
            id: AssetId::new("1").unwrap(),
            kind: AssetKind::Driver,
            name: "Max Verstappen".into(),
            tla: Some(tla("VER")),
            team_id: TeamId::new("9").unwrap(),
            team_name: "Red Bull".into(),
            history: vec![GamedaySnapshot {
                gameday: Gameday::new(1).unwrap(),
                active: true,
                price: 29.5,
                old_price: 29.0,
                ownership: 42.1,
                points: 33.0,
                quali_pts: 10.0,
                sprint_pts: 0.0,
                race_pts: 23.0,
                stats: ComponentStats {
                    fastest_lap_pts: 10.0,
                    value_for_money: 1.12,
                    ..ComponentStats::default()
                },
            }],
        };
        let constructor = Asset {
            id: AssetId::new("9").unwrap(),
            kind: AssetKind::Constructor,
            name: "Red Bull".into(),
            tla: None,
            team_id: TeamId::new("9").unwrap(),
            team_name: String::new(),
            history: vec![],
        };
        let synced = Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap()
            + chrono::Duration::nanoseconds(123_456_789);
        Season::new(2026, synced, vec![r1, r2], vec![driver, constructor]).unwrap()
    }

    #[test]
    fn given_a_season_when_saved_and_loaded_then_the_loaded_season_equals_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let repo = JsonSeasonRepository::new(dir.path().join("nested").join("season.json"));
        let season = sample_season();

        repo.save(&season).unwrap();
        let loaded = repo.load().unwrap().unwrap();

        assert_eq!(loaded, season);
        assert!(repo.path().exists());
        assert!(
            !dir.path().join("nested").join("season.json.tmp").exists(),
            "the temp file is renamed away"
        );
    }

    #[test]
    fn given_a_saved_season_when_the_json_is_inspected_then_it_uses_the_go_field_names() {
        let dir = tempfile::tempdir().unwrap();
        let repo = JsonSeasonRepository::new(dir.path().join("season.json"));
        repo.save(&sample_season()).unwrap();

        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(repo.path()).unwrap()).unwrap();

        assert_eq!(json["season"], 2026);
        assert_eq!(json["synced_at"], "2026-03-09T12:00:00.123456789Z");
        let r1 = &json["rounds"][0];
        assert_eq!(r1["round"], 1);
        assert_eq!(r1["date"], "2026-03-08");
        assert_eq!(r1["sessions"]["Qualifying"], "2026-03-07T05:00:00Z");
        assert_eq!(r1["race"]["VER"]["fastest_lap"], true);
        assert!(
            r1["race"]["HAM"].get("fastest_lap").is_none(),
            "fastest_lap is omitted when false"
        );
        assert!(r1.get("sprint").is_none(), "an empty sprint map is omitted");
        let r2 = &json["rounds"][1];
        assert!(
            r2.get("points_seen_at").is_none(),
            "an absent points_seen_at is omitted"
        );
        assert_eq!(r2["date"], "");
        assert_eq!(r2["grid"]["VER"], 3);
        let constructor = &json["assets"][1];
        assert_eq!(constructor["kind"], "constructor");
        assert!(constructor.get("tla").is_none(), "an absent tla is omitted");
        assert_eq!(
            json["assets"][0]["history"][0]["stats"]["value_for_money"],
            1.12
        );
    }

    #[test]
    fn given_no_file_when_loaded_then_there_is_no_season() {
        let dir = tempfile::tempdir().unwrap();
        let repo = JsonSeasonRepository::new(dir.path().join("absent.json"));

        assert_eq!(repo.load().unwrap(), None);
    }

    #[test]
    fn given_a_malformed_file_when_loaded_then_the_error_names_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.json");
        std::fs::write(&path, "{ not json").unwrap();
        let repo = JsonSeasonRepository::new(&path);

        let err = repo.load().unwrap_err();

        assert!(err.to_string().contains("broken.json"), "{err}");
    }

    #[test]
    fn given_a_file_with_the_go_zero_time_when_loaded_then_points_seen_at_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("season.json");
        std::fs::write(
            &path,
            r#"{"season": 2026, "synced_at": "2026-09-25T16:48:15.029739602Z",
                "rounds": [{"round": 1, "name": "X", "circuit_id": "x", "date": "", "has_sprint": false,
                            "has_results": false, "points_seen_at": "0001-01-01T00:00:00Z",
                            "sessions": {"Race": "2026-03-08T04:00:00Z", "Bogus": "2026-03-08T04:00:00Z"}}],
                "assets": []}"#,
        )
        .unwrap();

        let season = JsonSeasonRepository::new(&path).load().unwrap().unwrap();

        let round = &season.rounds()[0];
        assert_eq!(round.points_seen_at, None);
        assert_eq!(round.date, None);
        assert_eq!(
            round.sessions.len(),
            1,
            "an unknown session label is skipped"
        );
        assert!(round.sessions.contains_key(&Session::Race));
    }

    #[test]
    fn given_a_file_with_an_unknown_asset_kind_when_loaded_then_it_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("season.json");
        std::fs::write(
            &path,
            r#"{"season": 2026, "synced_at": "2026-09-25T16:48:15Z", "rounds": [],
                "assets": [{"id": "1", "kind": "robot", "name": "X", "team_id": "1", "team_name": "T", "history": []}]}"#,
        )
        .unwrap();

        let err = JsonSeasonRepository::new(&path).load().unwrap_err();

        assert!(err.to_string().contains("robot"), "{err}");
    }

    #[test]
    fn given_the_go_fixture_when_loaded_then_the_season_matches_the_known_shape() {
        let repo = JsonSeasonRepository::new(FIXTURE);

        let season = repo.load().unwrap().unwrap();

        assert_eq!(season.year(), 2026);
        assert_eq!(season.rounds().len(), 23);
        assert_eq!(season.assets().len(), 35);
        let r2 = season.round(RoundNumber::new(2).unwrap()).unwrap();
        assert!(r2.has_sprint);
        assert!(!r2.sprint.is_empty());
        assert!(r2.has_results);
        let r15 = season.round(RoundNumber::new(15).unwrap()).unwrap();
        assert!(!r15.grid.is_empty());
        assert!(!r15.has_results);
        assert!(r15.race.is_empty());
        assert!(
            r15.points_seen_at.is_none(),
            "the Go zero time reads as None"
        );
        let r1 = season.round(RoundNumber::new(1).unwrap()).unwrap();
        assert_eq!(r1.date, NaiveDate::from_ymd_opt(2026, 3, 8));
        assert_eq!(r1.sessions.len(), 5);
        assert!(r1.race[&tla("VER")].fastest_lap);
        assert!(r1.race[&tla("ALO")].dnf);
        assert!(r1.points_seen_at.is_some());
        let gasly = season.find_asset("GAS").unwrap();
        assert_eq!(gasly.name, "Pierre Gasly");
        assert_eq!(gasly.history[0].stats.value_for_money, 0.92);
    }

    #[test]
    fn given_the_go_fixture_when_saved_and_loaded_again_then_the_season_is_unchanged() {
        let original = JsonSeasonRepository::new(FIXTURE).load().unwrap().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let copy = JsonSeasonRepository::new(dir.path().join("copy.json"));

        copy.save(&original).unwrap();
        let reloaded = copy.load().unwrap().unwrap();

        assert_eq!(reloaded, original);
    }
}
