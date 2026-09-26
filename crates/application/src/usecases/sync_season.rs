//! Download the season from the public sources and join it.

use std::collections::BTreeMap;
use std::sync::Arc;

use overcut_domain::season::{Asset, GamedaySnapshot, RaceRow, Round, Season, SeasonRepository};
use overcut_domain::shared::{AssetId, Gameday, RoundNumber, TeamId, Tla};
use tracing::warn;

use crate::dto::SyncView;
use crate::ports::{Clock, FantasyFeedGateway, RaceDataGateway, StartingGridGateway};
use crate::AppError;

/// The external sources a sync reads.
pub struct SyncSources {
    /// The calendar and the classifications.
    pub race_data: Arc<dyn RaceDataGateway>,
    /// Prices, ownership, and official points.
    pub fantasy_feed: Arc<dyn FantasyFeedGateway>,
    /// The official starting grid.
    pub starting_grid: Arc<dyn StartingGridGateway>,
    /// The clock.
    pub clock: Arc<dyn Clock>,
}

/// Downloads a season, joins the sources, and stores the result.
pub struct SyncSeason {
    sources: SyncSources,
    store: Arc<dyn SeasonRepository>,
}

impl SyncSeason {
    /// Builds the use case.
    pub fn new(sources: SyncSources, store: Arc<dyn SeasonRepository>) -> Self {
        Self { sources, store }
    }

    /// Syncs the season. With `when_due` set, the sync runs only when the
    /// stored season says one is due. Returns the outcome and the new
    /// season when a sync ran.
    pub async fn execute(
        &self,
        year: u16,
        when_due: bool,
    ) -> Result<(SyncView, Option<Season>), AppError> {
        let now = self.sources.clock.now();
        let previous = self.store.load()?;
        let reason = match &previous {
            Some(prev) if when_due => {
                let decision = prev.sync_due(now);
                if !decision.due {
                    return Ok((
                        SyncView {
                            synced: false,
                            reason: decision.reason,
                            rounds: prev.rounds().len(),
                            completed_rounds: prev.completed_rounds().count(),
                            assets: prev.assets().len(),
                            provisional_rounds: provisional(prev, now),
                            next_session: prev.next_session(now).map(|(r, s, t)| {
                                format!("round {r} {} at {}", s.as_str(), t.to_rfc3339())
                            }),
                        },
                        None,
                    ));
                }
                decision.reason
            }
            _ => "requested".to_string(),
        };

        let mut season = self.download(year).await?;
        season.carry_points_seen(previous.as_ref(), now);
        self.store.save(&season)?;
        let view = SyncView {
            synced: true,
            reason,
            rounds: season.rounds().len(),
            completed_rounds: season.completed_rounds().count(),
            assets: season.assets().len(),
            provisional_rounds: provisional(&season, now),
            next_session: season
                .next_session(now)
                .map(|(r, s, t)| format!("round {r} {} at {}", s.as_str(), t.to_rfc3339())),
        };
        Ok((view, Some(season)))
    }

    /// Downloads and joins the sources into a season.
    async fn download(&self, year: u16) -> Result<Season, AppError> {
        let rd = &self.sources.race_data;
        let calendar = rd.calendar(year).await?;
        let qualifying = rd.qualifying(year).await?;
        let sprints = rd.sprints(year).await?;
        let races = rd.races(year).await?;

        let mut rounds: BTreeMap<u32, Round> = BTreeMap::new();
        for entry in calendar {
            let Ok(number) = RoundNumber::new(entry.round) else {
                continue;
            };
            let mut r = Round::new(number, entry.name);
            r.circuit_id = entry.circuit_id;
            r.date = entry.date;
            r.has_sprint = entry.has_sprint;
            r.sessions = entry.sessions.into_iter().collect();
            rounds.insert(entry.round, r);
        }
        for q in qualifying {
            if let Some(r) = rounds.get_mut(&q.round) {
                r.quali = q.positions.into_iter().collect();
            }
        }
        let to_row = |c: &crate::ports::ClassifiedCar| RaceRow {
            grid: c.grid,
            pos: c.position,
            dnf: !c.classified,
            fastest_lap: c.fastest_lap,
        };
        for s in sprints {
            if let Some(r) = rounds.get_mut(&s.round) {
                r.sprint = s.cars.iter().map(|c| (c.tla.clone(), to_row(c))).collect();
            }
        }
        for race in races {
            if let Some(r) = rounds.get_mut(&race.round) {
                r.has_results = true;
                r.race = race
                    .cars
                    .iter()
                    .map(|c| (c.tla.clone(), to_row(c)))
                    .collect();
            }
        }

        // The official starting grid, for rounds between qualifying and the
        // race. A failure here is not fatal: the grid is an enrichment.
        for r in rounds
            .values_mut()
            .filter(|r| r.has_quali() && !r.has_results)
        {
            match self
                .sources
                .starting_grid
                .starting_grid(year, &r.name)
                .await
            {
                Ok(Some(slots)) if slots.len() >= r.quali.len() => {
                    r.grid = slots.into_iter().map(|s| (s.tla, s.position)).collect();
                }
                Ok(Some(slots)) => warn!(
                    round = r.number.get(),
                    rows = slots.len(),
                    "starting grid does not cover the qualified field"
                ),
                Ok(None) => {}
                Err(e) => warn!(round = r.number.get(), error = %e, "starting grid unavailable"),
            }
        }

        // The feed publishes one document per gameday. Fetch until the feed
        // reports an unpublished gameday.
        let mut assets: BTreeMap<String, Asset> = BTreeMap::new();
        let mut order = Vec::new();
        let max_gameday = rounds.len() as u32 + 1;
        for g in 1..=max_gameday {
            let Some(doc) = self.sources.fantasy_feed.gameday(g).await? else {
                break;
            };
            let gameday = Gameday::new(g).map_err(AppError::from)?;
            for p in doc.players {
                if !assets.contains_key(&p.id) {
                    let id = AssetId::new(p.id.clone())?;
                    let team_id = TeamId::new(if p.team_id.is_empty() {
                        p.id.clone()
                    } else {
                        p.team_id.clone()
                    })?;
                    assets.insert(
                        p.id.clone(),
                        Asset {
                            id,
                            kind: p.kind,
                            name: p.name.clone(),
                            tla: p.tla.as_deref().and_then(|t| Tla::parse(t).ok()),
                            team_id,
                            // The feed leaves the team name blank on constructor rows.
                            team_name: if p.team_name.is_empty() {
                                p.name.clone()
                            } else {
                                p.team_name.clone()
                            },
                            history: Vec::new(),
                        },
                    );
                    order.push(p.id.clone());
                }
                let asset = assets.get_mut(&p.id).expect("asset just inserted");
                asset.history.push(GamedaySnapshot {
                    gameday,
                    active: p.active,
                    price: p.price,
                    old_price: p.old_price,
                    ownership: p.ownership,
                    points: p.points,
                    quali_pts: p.quali_pts,
                    sprint_pts: p.sprint_pts,
                    race_pts: p.race_pts,
                    stats: p.stats,
                });
            }
        }
        if assets.is_empty() {
            return Err(AppError::NoAssets);
        }
        let assets: Vec<Asset> = order
            .into_iter()
            .filter_map(|id| assets.remove(&id))
            .collect();
        Ok(Season::new(
            year,
            self.sources.clock.now(),
            rounds.into_values().collect(),
            assets,
        )?)
    }
}

fn provisional(season: &Season, now: chrono::DateTime<chrono::Utc>) -> Vec<u32> {
    season
        .rounds()
        .iter()
        .filter(|r| r.provisional(now))
        .map(|r| r.number.get())
        .collect()
}
