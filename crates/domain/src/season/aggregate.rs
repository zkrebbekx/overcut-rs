//! The `Season` aggregate root.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::shared::{AssetId, AssetKind, Gameday, RoundNumber};
use crate::DomainError;

use super::policy::{self, SyncDecision};
use super::{Asset, Round, Session};

/// One season of data: the calendar, the classifications, and the market
/// history of every asset.
///
/// The aggregate keeps two invariants: the rounds are in round order with
/// no duplicate number, and no two assets share an identifier.
#[derive(Debug, Clone, PartialEq)]
pub struct Season {
    year: u16,
    synced_at: DateTime<Utc>,
    rounds: Vec<Round>,
    assets: Vec<Asset>,
}

impl Season {
    /// Builds a season. Rounds are sorted by number. Duplicate round
    /// numbers and duplicate asset identifiers are rejected.
    pub fn new(
        year: u16,
        synced_at: DateTime<Utc>,
        mut rounds: Vec<Round>,
        assets: Vec<Asset>,
    ) -> Result<Self, DomainError> {
        rounds.sort_by_key(|r| r.number);
        if let Some(dup) = rounds.windows(2).find(|w| w[0].number == w[1].number) {
            return Err(DomainError::DuplicateRound(dup[0].number.get()));
        }
        let mut seen = std::collections::HashSet::new();
        if let Some(dup) = assets.iter().find(|a| !seen.insert(a.id.clone())) {
            return Err(DomainError::DuplicateAsset(dup.id.to_string()));
        }
        Ok(Self {
            year,
            synced_at,
            rounds,
            assets,
        })
    }

    /// The season year.
    pub fn year(&self) -> u16 {
        self.year
    }

    /// When the season was last downloaded.
    pub fn synced_at(&self) -> DateTime<Utc> {
        self.synced_at
    }

    /// Every round in calendar order.
    pub fn rounds(&self) -> &[Round] {
        &self.rounds
    }

    /// Every asset in feed order.
    pub fn assets(&self) -> &[Asset] {
        &self.assets
    }

    /// The driver assets.
    pub fn drivers(&self) -> impl Iterator<Item = &Asset> {
        self.assets.iter().filter(|a| a.kind == AssetKind::Driver)
    }

    /// The constructor assets.
    pub fn constructors(&self) -> impl Iterator<Item = &Asset> {
        self.assets
            .iter()
            .filter(|a| a.kind == AssetKind::Constructor)
    }

    /// The asset with the given identifier.
    pub fn asset(&self, id: &AssetId) -> Option<&Asset> {
        self.assets.iter().find(|a| &a.id == id)
    }

    /// The rounds that have a race result, in order.
    pub fn completed_rounds(&self) -> impl Iterator<Item = &Round> {
        self.rounds.iter().filter(|r| r.has_results)
    }

    /// The round with the given number.
    pub fn round(&self, number: RoundNumber) -> Option<&Round> {
        self.rounds.iter().find(|r| r.number == number)
    }

    /// The first round without a race result, or `None` when the season is
    /// complete.
    pub fn next_round(&self) -> Option<&Round> {
        self.rounds.iter().find(|r| !r.has_results)
    }

    /// The last round with a race result.
    pub fn latest_completed_round(&self) -> Option<&Round> {
        self.completed_rounds().last()
    }

    /// Resolves a target round: the given number, or the next round without
    /// a result when `number` is `None`.
    pub fn resolve_round(&self, number: Option<RoundNumber>) -> Result<&Round, DomainError> {
        match number {
            Some(n) => self.round(n).ok_or(DomainError::UnknownRound(n.get())),
            None => self.next_round().ok_or(DomainError::SeasonComplete),
        }
    }

    /// Resolves a finished round: the given number, or the latest finished
    /// round when `number` is `None`.
    pub fn resolve_finished_round(
        &self,
        number: Option<RoundNumber>,
    ) -> Result<&Round, DomainError> {
        match number {
            Some(n) => match self.round(n) {
                Some(r) if r.has_results => Ok(r),
                _ => Err(DomainError::RoundNotFinished(n.get())),
            },
            None => self
                .latest_completed_round()
                .ok_or(DomainError::NoCompletedRounds),
        }
    }

    /// The highest gameday in any asset history.
    pub fn latest_gameday(&self) -> Option<Gameday> {
        self.assets
            .iter()
            .filter_map(|a| a.latest().map(|h| h.gameday))
            .max()
    }

    /// Reports whether the asset can join a team now: the asset is active
    /// in the latest gameday snapshot. A driver that lost the seat in a
    /// mid-season swap stays in the history but is not selectable.
    pub fn selectable(&self, asset: &Asset) -> bool {
        match (asset.latest(), self.latest_gameday()) {
            (Some(h), Some(latest)) => h.gameday == latest && h.active,
            _ => false,
        }
    }

    /// The selectable assets, in feed order.
    pub fn selectable_assets(&self) -> impl Iterator<Item = &Asset> {
        self.assets.iter().filter(|a| self.selectable(a))
    }

    /// Finds an asset from a user token: a driver code such as `VER`, or a
    /// case-insensitive prefix of a constructor name such as `McLaren`.
    pub fn find_asset(&self, token: &str) -> Result<&Asset, DomainError> {
        let token = token.trim();
        self.assets
            .iter()
            .find(|a| match a.kind {
                AssetKind::Driver => a.has_tla(token),
                AssetKind::Constructor => a.name.to_lowercase().starts_with(&token.to_lowercase()),
            })
            .ok_or_else(|| DomainError::UnknownAsset(token.to_string()))
    }

    /// Returns the next scheduled session after `now`.
    pub fn next_session(
        &self,
        now: DateTime<Utc>,
    ) -> Option<(RoundNumber, Session, DateTime<Utc>)> {
        self.rounds
            .iter()
            .flat_map(|r| r.sessions.iter().map(move |(s, t)| (r.number, *s, *t)))
            .filter(|(_, _, start)| *start > now)
            .min_by_key(|(_, _, start)| *start)
    }

    /// Decides whether a sync should run now, and why. A sync is due when
    /// a session ended within [`policy::DUE_WINDOW`], when a round has
    /// qualified but its official grid is not in yet and the race has not
    /// started, when a sprint has run without a result, when a finished
    /// round's points are provisional and the last check is older than
    /// [`policy::SETTLE_INTERVAL`], or when the data is older than
    /// [`policy::MAX_AGE`].
    pub fn sync_due(&self, now: DateTime<Utc>) -> SyncDecision {
        for r in &self.rounds {
            for (session, start) in &r.sessions {
                let end = *start + session.length();
                if now >= end && now < end + policy::DUE_WINDOW {
                    let ago = now - end;
                    return SyncDecision::due(format!(
                        "round {} {} ended {} minutes ago",
                        r.number,
                        session.as_str(),
                        ago.num_minutes()
                    ));
                }
            }
            let race_start = r.sessions.get(&Session::Race).copied();
            let before_race = race_start.is_none_or(|race| now < race);
            if r.has_quali() && r.grid.is_empty() && !r.has_results && before_race {
                return SyncDecision::due(format!(
                    "round {} has qualified and the official grid is not in yet",
                    r.number
                ));
            }
            if r.has_sprint && !r.has_sprint_result() && !r.has_results {
                let sprint_over = r
                    .sessions
                    .get(&Session::Sprint)
                    .is_some_and(|s| now > *s + Session::Sprint.length());
                if sprint_over && before_race {
                    return SyncDecision::due(format!(
                        "round {} sprint has run and its result is not in yet",
                        r.number
                    ));
                }
            }
        }
        // After a race, keep checking for the finalised points.
        for r in &self.rounds {
            let Some(race) = r.sessions.get(&Session::Race) else {
                continue;
            };
            if !r.has_results {
                continue;
            }
            let end = *race + Session::Race.length();
            let since_sync = now - self.synced_at;
            if now > end
                && now < end + policy::SETTLE_WINDOW
                && r.provisional(now)
                && since_sync >= policy::SETTLE_INTERVAL
            {
                return SyncDecision::due(format!(
                    "round {} points are provisional and the last check was {} minutes ago",
                    r.number,
                    since_sync.num_minutes()
                ));
            }
        }
        let age = now - self.synced_at;
        if age > policy::MAX_AGE {
            return SyncDecision::due(format!("dataset is {} hours old", age.num_hours()));
        }
        SyncDecision::not_due("no session ended recently and the dataset is fresh")
    }

    /// Summarises every asset's official points for one gameday so two
    /// syncs can be compared.
    fn points_key(&self, gameday: Gameday) -> BTreeMap<AssetId, [f64; 4]> {
        self.assets
            .iter()
            .filter_map(|a| {
                a.at(gameday).map(|h| {
                    (
                        a.id.clone(),
                        [h.points, h.quali_pts, h.sprint_pts, h.race_pts],
                    )
                })
            })
            .collect()
    }

    /// Sets each finished round's first-seen time: the previous season's
    /// value when the round's points are unchanged since that sync,
    /// otherwise `now`. The game publishes provisional points on race day
    /// and finalises them within a day, so the first-seen time tells a
    /// provisional round from a final one.
    pub fn carry_points_seen(&mut self, previous: Option<&Season>, now: DateTime<Utc>) {
        for i in 0..self.rounds.len() {
            if !self.rounds[i].has_results {
                continue;
            }
            let number = self.rounds[i].number;
            let mut seen = now;
            if let Some(prev) = previous {
                if let Some(pr) = prev.round(number) {
                    let unchanged = pr.has_results
                        && pr.points_seen_at.is_some()
                        && prev.points_key(number.gameday()) == self.points_key(number.gameday());
                    if unchanged {
                        seen = pr.points_seen_at.unwrap_or(now);
                    }
                }
            }
            self.rounds[i].points_seen_at = Some(seen);
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;
    use crate::season::GamedaySnapshot;
    use crate::shared::TeamId;

    fn round(n: u32) -> Round {
        Round::new(RoundNumber::new(n).unwrap(), format!("R{n}"))
    }

    fn driver(id: &str, history: Vec<GamedaySnapshot>) -> Asset {
        Asset {
            id: AssetId::new(id).unwrap(),
            kind: AssetKind::Driver,
            name: id.to_string(),
            tla: None,
            team_id: TeamId::new("t").unwrap(),
            team_name: "T".into(),
            history,
        }
    }

    fn snap(g: u32, active: bool, points: f64) -> GamedaySnapshot {
        GamedaySnapshot {
            active,
            points,
            ..GamedaySnapshot::new(Gameday::new(g).unwrap(), 10.0)
        }
    }

    fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
    }

    mod given_a_calendar_with_qualifying_at_14_and_the_race_the_next_day {
        use super::*;

        fn season() -> (Season, DateTime<Utc>, DateTime<Utc>) {
            let quali = at(2026, 9, 5, 14);
            let race = at(2026, 9, 6, 13);
            let mut r = round(13);
            r.sessions.insert(Session::Qualifying, quali);
            r.sessions.insert(Session::Race, race);
            let s = Season::new(2026, quali - Duration::hours(2), vec![r], vec![]).unwrap();
            (s, quali, race)
        }

        #[test]
        fn when_the_clock_is_inside_qualifying_then_no_sync_is_due() {
            let (s, quali, _) = season();
            assert!(!s.sync_due(quali + Duration::minutes(30)).due);
        }

        #[test]
        fn when_qualifying_ended_twenty_minutes_ago_then_a_sync_is_due_and_names_the_session() {
            let (s, quali, _) = season();
            let d = s.sync_due(quali + Duration::minutes(80));
            assert!(d.due);
            assert!(d.reason.contains("Qualifying"), "{}", d.reason);
        }

        #[test]
        fn when_qualifying_ended_seven_hours_ago_and_the_data_is_nine_hours_old_then_no_sync_is_due(
        ) {
            let (s, quali, _) = season();
            assert!(!s.sync_due(quali + Duration::hours(8)).due);
        }

        #[test]
        fn when_the_race_ended_an_hour_ago_then_a_sync_is_due_for_the_race() {
            let (s, _, race) = season();
            let d = s.sync_due(race + Duration::hours(4));
            assert!(d.due);
            assert!(d.reason.contains("Race"));
        }

        #[test]
        fn when_the_data_is_two_days_old_then_a_sync_is_due_because_of_age() {
            let (s, _, race) = season();
            let d = s.sync_due(race + Duration::days(3));
            assert!(d.due);
            assert!(d.reason.contains("old"));
        }

        #[test]
        fn when_the_next_session_is_asked_before_qualifying_then_it_is_round_13_qualifying() {
            let (s, quali, _) = season();
            let (n, session, start) = s.next_session(quali - Duration::hours(1)).unwrap();
            assert_eq!(n.get(), 13);
            assert_eq!(session, Session::Qualifying);
            assert_eq!(start, quali);
        }
    }

    mod given_provisional_points {
        use super::*;

        fn race() -> DateTime<Utc> {
            at(2026, 9, 6, 13)
        }

        fn finished(seen: Option<DateTime<Utc>>) -> Round {
            let mut r = round(13);
            r.has_results = true;
            r.sessions.insert(Session::Race, race());
            r.points_seen_at = seen;
            r
        }

        #[test]
        fn when_six_hours_have_passed_then_the_points_are_provisional() {
            let seen = race() + Duration::hours(3);
            assert!(finished(Some(seen)).provisional(seen + Duration::hours(6)));
        }

        #[test]
        fn when_a_day_and_a_half_has_passed_then_the_points_are_final() {
            let seen = race() + Duration::hours(3);
            assert!(!finished(Some(seen)).provisional(seen + Duration::hours(36)));
        }

        #[test]
        fn when_first_synced_a_week_after_the_race_then_the_points_are_final_at_once() {
            let seen = race() + Duration::days(7);
            assert!(!finished(Some(seen)).provisional(seen + Duration::minutes(1)));
        }

        #[test]
        fn when_a_new_sync_has_unchanged_points_then_the_first_seen_time_carries_over() {
            let seen = race() + Duration::hours(3);
            let prev = Season::new(
                2026,
                seen,
                vec![finished(Some(seen))],
                vec![driver("a", vec![snap(13, true, 67.0)])],
            )
            .unwrap();
            let mut next = Season::new(
                2026,
                seen,
                vec![finished(None)],
                vec![driver("a", vec![snap(13, true, 67.0)])],
            )
            .unwrap();
            next.carry_points_seen(Some(&prev), seen + Duration::hours(20));
            assert_eq!(next.rounds()[0].points_seen_at, Some(seen));
        }

        #[test]
        fn when_a_new_sync_has_changed_points_then_the_clock_restarts() {
            let seen = race() + Duration::hours(3);
            let now = seen + Duration::hours(20);
            let prev = Season::new(
                2026,
                seen,
                vec![finished(Some(seen))],
                vec![driver("a", vec![snap(13, true, 67.0)])],
            )
            .unwrap();
            let mut next = Season::new(
                2026,
                seen,
                vec![finished(None)],
                vec![driver("a", vec![snap(13, true, 93.0)])],
            )
            .unwrap();
            next.carry_points_seen(Some(&prev), now);
            assert_eq!(next.rounds()[0].points_seen_at, Some(now));
            assert!(next.rounds()[0].provisional(now + Duration::hours(1)));
        }

        #[test]
        fn when_the_race_window_closed_and_seven_hours_passed_then_a_sync_is_due_for_final_points()
        {
            let seen = race() + Duration::hours(3);
            let s = Season::new(2026, seen, vec![finished(Some(seen))], vec![]).unwrap();
            let d = s.sync_due(race() + Duration::hours(10));
            assert!(d.due);
            assert!(d.reason.contains("provisional"));
        }

        #[test]
        fn when_the_last_check_was_an_hour_ago_then_no_sync_is_due_yet() {
            let seen = race() + Duration::hours(3);
            let s = Season::new(
                2026,
                race() + Duration::hours(9),
                vec![finished(Some(seen))],
                vec![],
            )
            .unwrap();
            assert!(!s.sync_due(race() + Duration::hours(10)).due);
        }
    }

    mod given_a_mid_season_seat_swap {
        use super::*;

        fn season() -> Season {
            Season::new(
                2026,
                Utc::now(),
                vec![],
                vec![
                    driver("old", vec![snap(1, true, 0.0), snap(5, false, 0.0)]),
                    driver("new", vec![snap(5, true, 0.0)]),
                    driver("gone", vec![snap(3, true, 0.0)]),
                ],
            )
            .unwrap()
        }

        #[test]
        fn when_the_selectable_filter_runs_then_only_the_replacement_is_selectable() {
            let s = season();
            assert!(s.selectable(&s.assets()[1]));
            assert!(!s.selectable(&s.assets()[0]));
            assert!(!s.selectable(&s.assets()[2]));
        }
    }

    mod given_duplicate_rounds {
        use super::*;

        #[test]
        fn when_the_season_is_built_then_it_is_rejected() {
            let err = Season::new(2026, Utc::now(), vec![round(1), round(1)], vec![]).unwrap_err();
            assert_eq!(err, DomainError::DuplicateRound(1));
        }
    }
}
