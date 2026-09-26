//! A small synthetic season for the projection tests: six drivers on three
//! teams, five completed rounds, with team 0 fastest and team 2 slowest.

use chrono::Utc;

use crate::season::{Asset, GamedaySnapshot, RaceRow, Round, Season};
use crate::shared::{AssetId, AssetKind, Gameday, RoundNumber, TeamId, Tla};

/// Builds the synthetic season.
pub fn synthetic_season() -> Season {
    let tlas = ["AAA", "AAB", "BBA", "BBB", "CCA", "CCB"];
    let mut rounds = Vec::new();
    for n in 1..=5 {
        let mut r = Round::new(RoundNumber::new(n).unwrap(), format!("R{n}"));
        r.has_results = true;
        for (i, tla) in tlas.iter().enumerate() {
            let pos = i as u32 + 1;
            let code = Tla::parse(tla).unwrap();
            r.quali.insert(code.clone(), pos);
            r.race.insert(
                code,
                RaceRow {
                    grid: pos,
                    pos,
                    ..Default::default()
                },
            );
        }
        rounds.push(r);
    }
    let mut assets = Vec::new();
    for (i, tla) in tlas.iter().enumerate() {
        assets.push(Asset {
            id: AssetId::new(format!("d{i}")).unwrap(),
            kind: AssetKind::Driver,
            name: (*tla).to_string(),
            tla: Some(Tla::parse(tla).unwrap()),
            team_id: TeamId::new(format!("t{}", i / 2)).unwrap(),
            team_name: format!("Team{}", i / 2),
            history: (1..=5)
                .map(|g| GamedaySnapshot {
                    points: f64::from(30 - i as i32 * 5),
                    ..GamedaySnapshot::new(Gameday::new(g).unwrap(), 10.0)
                })
                .collect(),
        });
    }
    for t in 0..3 {
        assets.push(Asset {
            id: AssetId::new(format!("c{t}")).unwrap(),
            kind: AssetKind::Constructor,
            name: format!("Team{t}"),
            tla: None,
            team_id: TeamId::new(format!("t{t}")).unwrap(),
            team_name: format!("Team{t}"),
            history: (1..=5)
                .map(|g| GamedaySnapshot {
                    points: f64::from(60 - t * 15),
                    ..GamedaySnapshot::new(Gameday::new(g).unwrap(), 20.0)
                })
                .collect(),
        });
    }
    Season::new(2026, Utc::now(), rounds, assets).unwrap()
}
