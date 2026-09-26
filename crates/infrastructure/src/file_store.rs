//! The JSON file store of the `Season` aggregate.
//!
//! The store keeps the whole season in one JSON file. The file format is
//! the format of the original Go toolkit, so a file that the Go toolkit
//! wrote loads without change. The `overcut-codec` crate maps the JSON to
//! and from the domain; this module only does the file I/O.
//!
//! A save writes to `<path>.tmp` and then renames the file into place, so a
//! reader never sees a partial file.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use overcut_codec::CodecError;
use overcut_domain::season::{RepositoryError, Season, SeasonRepository};
use thiserror::Error;

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
        overcut_codec::decode_bytes(&bytes)
            .map(Some)
            .map_err(|source| {
                RepositoryError::new(StoreError::Parse {
                    path: self.path.clone(),
                    source,
                })
            })
    }

    fn save(&self, season: &Season) -> Result<(), RepositoryError> {
        if let Some(parent) = self.path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| self.write_error(e))?;
        }
        let json = overcut_codec::encode(season).map_err(|source| {
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
    Parse { path: PathBuf, source: CodecError },
    #[error("cannot encode {}: {source}", path.display())]
    Encode { path: PathBuf, source: CodecError },
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, TimeZone, Utc};
    use overcut_domain::season::{Asset, ComponentStats, GamedaySnapshot, RaceRow, Round, Session};
    use overcut_domain::shared::{AssetId, AssetKind, Gameday, RoundNumber, TeamId, Tla};

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
