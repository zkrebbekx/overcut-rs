//! One fantasy asset and its gameday history.

use crate::shared::{AssetId, AssetKind, Gameday, TeamId, Tla};

/// The season-to-date component breakdown that the fantasy feed publishes
/// per asset. The values are cumulative through the gameday.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ComponentStats {
    /// Fastest-lap points to date.
    pub fastest_lap_pts: f64,
    /// Driver-of-the-day points to date.
    pub dotd_pts: f64,
    /// Overtaking points to date.
    pub overtaking_pts: f64,
    /// Q3 appearance points to date.
    pub q3_finishes_pts: f64,
    /// Position points to date.
    pub position_pts: f64,
    /// Positions gained or lost to date.
    pub pos_gained_lost: f64,
    /// DNF and disqualification points to date.
    pub dnf_pts: f64,
    /// The feed's value-for-money ratio.
    pub value_for_money: f64,
    /// Top-ten race position points to date.
    pub top10_race_pts: f64,
    /// Top-eight sprint position points to date.
    pub top8_sprint_pts: f64,
}

/// One asset's feed snapshot for one gameday: the market state before the
/// round and the official points once the round is scored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GamedaySnapshot {
    /// The gameday.
    pub gameday: Gameday,
    /// The asset can join a team in this gameday.
    pub active: bool,
    /// The price in millions.
    pub price: f64,
    /// The price before this gameday's change, in millions.
    pub old_price: f64,
    /// The share of teams that hold the asset, in percent.
    pub ownership: f64,
    /// The official points for the gameday.
    pub points: f64,
    /// The official qualifying points.
    pub quali_pts: f64,
    /// The official sprint points.
    pub sprint_pts: f64,
    /// The official race points.
    pub race_pts: f64,
    /// The cumulative component stats.
    pub stats: ComponentStats,
}

impl GamedaySnapshot {
    /// Builds a snapshot with the market fields set and the points at zero.
    pub fn new(gameday: Gameday, price: f64) -> Self {
        Self {
            gameday,
            active: true,
            price,
            old_price: price,
            ownership: 0.0,
            points: 0.0,
            quali_pts: 0.0,
            sprint_pts: 0.0,
            race_pts: 0.0,
            stats: ComponentStats::default(),
        }
    }

    /// The price change that applied at this gameday, in millions.
    pub fn price_change(&self) -> f64 {
        self.price - self.old_price
    }
}

/// One fantasy asset with its full gameday history.
#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    /// The game's identifier.
    pub id: AssetId,
    /// Driver or constructor.
    pub kind: AssetKind,
    /// The display name.
    pub name: String,
    /// The driver code. A constructor may carry one too.
    pub tla: Option<Tla>,
    /// The team the asset belongs to.
    pub team_id: TeamId,
    /// The team's display name.
    pub team_name: String,
    /// One snapshot per gameday, in gameday order.
    pub history: Vec<GamedaySnapshot>,
}

impl Asset {
    /// Returns the most recent gameday snapshot, or `None` when the asset
    /// has no history.
    pub fn latest(&self) -> Option<&GamedaySnapshot> {
        self.history.last()
    }

    /// Returns the snapshot for one gameday.
    pub fn at(&self, gameday: Gameday) -> Option<&GamedaySnapshot> {
        self.history.iter().find(|h| h.gameday == gameday)
    }

    /// Returns the team name, or the asset's own name for a constructor
    /// recorded without one.
    pub fn team_label(&self) -> &str {
        if self.team_name.is_empty() {
            &self.name
        } else {
            &self.team_name
        }
    }

    /// Reports whether the asset's code matches, case-insensitively.
    pub fn has_tla(&self, code: &str) -> bool {
        self.tla
            .as_ref()
            .is_some_and(|t| t.as_str().eq_ignore_ascii_case(code.trim()))
    }

    /// The official points of the gameday before `gameday`, or zero.
    pub fn previous_points(&self, gameday: Gameday) -> f64 {
        gameday
            .previous()
            .and_then(|g| self.at(g))
            .map_or(0.0, |h| h.points)
    }

    /// The mean official points over every gameday before `gameday`, or
    /// zero with no history.
    pub fn season_mean_before(&self, gameday: Gameday) -> f64 {
        let before: Vec<f64> = self
            .history
            .iter()
            .filter(|h| h.gameday < gameday)
            .map(|h| h.points)
            .collect();
        if before.is_empty() {
            0.0
        } else {
            before.iter().sum::<f64>() / before.len() as f64
        }
    }

    /// The per-gameday increase of a cumulative season-to-date statistic.
    pub fn component_deltas(&self, get: impl Fn(&ComponentStats) -> f64) -> Vec<(Gameday, f64)> {
        let mut prev = 0.0;
        self.history
            .iter()
            .map(|h| {
                let cur = get(&h.stats);
                let delta = cur - prev;
                prev = cur;
                (h.gameday, delta)
            })
            .collect()
    }
}
