//! One calendar round and its results.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, NaiveDate, Utc};

use crate::shared::{RoundNumber, Tla};

use super::policy;

/// A session of a race weekend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Session {
    /// First practice.
    FP1,
    /// Second practice.
    FP2,
    /// Third practice.
    FP3,
    /// Sprint qualifying (sprint weekends only).
    SprintQualifying,
    /// The sprint race (sprint weekends only).
    Sprint,
    /// Qualifying.
    Qualifying,
    /// The grand prix.
    Race,
}

impl Session {
    /// Every session in weekend order.
    pub const ALL: [Self; 7] = [
        Self::FP1,
        Self::FP2,
        Self::FP3,
        Self::SprintQualifying,
        Self::Sprint,
        Self::Qualifying,
        Self::Race,
    ];

    /// The label the data file and the API use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FP1 => "FP1",
            Self::FP2 => "FP2",
            Self::FP3 => "FP3",
            Self::SprintQualifying => "SprintQualifying",
            Self::Sprint => "Sprint",
            Self::Qualifying => "Qualifying",
            Self::Race => "Race",
        }
    }

    /// Parses a label. Returns `None` for an unknown label.
    pub fn parse(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == label)
    }

    /// The planned duration of the session, with slack for a delay. Data
    /// providers publish a classification after the session ends.
    pub fn length(self) -> Duration {
        match self {
            Self::Race => Duration::hours(3),
            _ => Duration::hours(1),
        }
    }
}

/// One driver's race or sprint outcome.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RaceRow {
    /// The grid slot. Zero means a pit-lane start.
    pub grid: u32,
    /// The classified position. Meaningful only when `dnf` is false.
    pub pos: u32,
    /// The car did not classify: retired, disqualified, or withdrawn.
    pub dnf: bool,
    /// The car set the fastest lap.
    pub fastest_lap: bool,
}

/// The sprint classification and grid of one round, as orders from P1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SprintOrder {
    /// The classification with the retired cars last.
    pub finish: Vec<Tla>,
    /// The sprint grid with pit-lane starters last.
    pub grid: Vec<Tla>,
    /// The cars that did not classify.
    pub dnf: BTreeSet<Tla>,
}

/// One round: the schedule entry and the results as far as they are known.
#[derive(Debug, Clone, PartialEq)]
pub struct Round {
    /// The round number.
    pub number: RoundNumber,
    /// The race name, such as "Italian Grand Prix".
    pub name: String,
    /// The circuit identifier of the classification source.
    pub circuit_id: String,
    /// The race date.
    pub date: Option<NaiveDate>,
    /// The weekend has a sprint.
    pub has_sprint: bool,
    /// The race classification is in.
    pub has_results: bool,
    /// The scheduled start of each session, in UTC.
    pub sessions: BTreeMap<Session, DateTime<Utc>>,
    /// The sync time at which the round's current official points were
    /// first observed. See [`Round::provisional`].
    pub points_seen_at: Option<DateTime<Utc>>,
    /// The final qualifying position per driver.
    pub quali: BTreeMap<Tla, u32>,
    /// The official starting position with penalties applied, per driver.
    /// Set from the official grid page once that is published.
    pub grid: BTreeMap<Tla, u32>,
    /// The race row per driver.
    pub race: BTreeMap<Tla, RaceRow>,
    /// The sprint row per driver.
    pub sprint: BTreeMap<Tla, RaceRow>,
}

impl Round {
    /// Builds an empty round with the given number and name.
    pub fn new(number: RoundNumber, name: impl Into<String>) -> Self {
        Self {
            number,
            name: name.into(),
            circuit_id: String::new(),
            date: None,
            has_sprint: false,
            has_results: false,
            sessions: BTreeMap::new(),
            points_seen_at: None,
            quali: BTreeMap::new(),
            grid: BTreeMap::new(),
            race: BTreeMap::new(),
            sprint: BTreeMap::new(),
        }
    }

    /// The official qualifying classification is in.
    pub fn has_quali(&self) -> bool {
        !self.quali.is_empty()
    }

    /// The sprint classification is in.
    pub fn has_sprint_result(&self) -> bool {
        !self.sprint.is_empty()
    }

    /// Returns the qualifying classification as codes from P1, or `None`
    /// when the round has no qualifying result yet.
    pub fn quali_order(&self) -> Option<Vec<Tla>> {
        if self.quali.is_empty() {
            return None;
        }
        let mut out: Vec<&Tla> = self.quali.keys().collect();
        out.sort_by_key(|tla| (self.quali[*tla], (*tla).clone()));
        Some(out.into_iter().cloned().collect())
    }

    /// Returns the official starting grid as codes from P1. Before the race
    /// the order comes from the published grid; after the race it comes
    /// from the race classification, which records each car's grid slot.
    /// A pit-lane start (slot 0) goes to the back. Returns `None` when
    /// neither is known.
    pub fn grid_order(&self) -> Option<Vec<Tla>> {
        let grid: BTreeMap<Tla, u32> = if self.grid.is_empty() {
            let mut back = self.race.len() as u32 + 1;
            self.race
                .iter()
                .map(|(tla, row)| {
                    let slot = if row.grid > 0 {
                        row.grid
                    } else {
                        back += 1;
                        back - 1
                    };
                    (tla.clone(), slot)
                })
                .collect()
        } else {
            self.grid.clone()
        };
        if grid.is_empty() {
            return None;
        }
        let mut out: Vec<Tla> = grid.keys().cloned().collect();
        out.sort_by_key(|tla| (grid[tla], tla.clone()));
        Some(out)
    }

    /// Returns the sprint classification, grid, and retired set, or `None`
    /// when the sprint has not run.
    pub fn sprint_order(&self) -> Option<SprintOrder> {
        if self.sprint.is_empty() {
            return None;
        }
        let dnf: BTreeSet<Tla> = self
            .sprint
            .iter()
            .filter(|(_, r)| r.dnf)
            .map(|(t, _)| t.clone())
            .collect();
        let mut finish: Vec<Tla> = self.sprint.keys().cloned().collect();
        finish.sort_by_key(|tla| {
            let row = self.sprint[tla];
            (row.dnf, row.pos, tla.clone())
        });
        let back = self.sprint.len() as u32 + 1;
        let slot = |tla: &Tla| {
            let g = self.sprint[tla].grid;
            if g > 0 {
                g
            } else {
                back
            }
        };
        let mut grid: Vec<Tla> = self.sprint.keys().cloned().collect();
        grid.sort_by_key(|tla| (slot(tla), tla.clone()));
        Some(SprintOrder { finish, grid, dnf })
    }

    /// Reports whether a finished round's official points may still
    /// change: the race ended recently and the current values have not
    /// stood for [`policy::STABLE_WINDOW`]. A round first synced long after
    /// its race is final at once.
    pub fn provisional(&self, now: DateTime<Utc>) -> bool {
        let Some(seen) = self.points_seen_at else {
            return false;
        };
        if !self.has_results {
            return false;
        }
        if let Some(race) = self.sessions.get(&Session::Race) {
            if seen - *race > policy::LATE_SYNC_IS_FINAL {
                return false;
            }
        }
        now - seen < policy::STABLE_WINDOW
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tla(s: &str) -> Tla {
        Tla::parse(s).unwrap()
    }

    mod given_a_round_with_a_qualifying_classification {
        use super::*;

        #[test]
        fn when_the_order_is_requested_then_the_codes_come_from_p1() {
            let mut r = Round::new(RoundNumber::new(1).unwrap(), "Test");
            r.quali = [(tla("HAM"), 3), (tla("GAS"), 1), (tla("RUS"), 2)]
                .into_iter()
                .collect();
            assert_eq!(
                r.quali_order(),
                Some(vec![tla("GAS"), tla("RUS"), tla("HAM")])
            );
        }
    }

    mod given_a_round_without_qualifying {
        use super::*;

        #[test]
        fn when_the_order_is_requested_then_there_is_none() {
            assert_eq!(
                Round::new(RoundNumber::new(1).unwrap(), "Test").quali_order(),
                None
            );
        }
    }

    mod given_a_raced_round_with_a_pit_lane_start {
        use super::*;

        #[test]
        fn when_the_grid_order_is_requested_then_the_pit_lane_car_goes_last() {
            let mut r = Round::new(RoundNumber::new(1).unwrap(), "Test");
            r.race = [
                (
                    tla("AAA"),
                    RaceRow {
                        grid: 2,
                        pos: 1,
                        ..Default::default()
                    },
                ),
                (
                    tla("BBB"),
                    RaceRow {
                        grid: 0,
                        pos: 2,
                        ..Default::default()
                    },
                ),
                (
                    tla("CCC"),
                    RaceRow {
                        grid: 1,
                        pos: 3,
                        ..Default::default()
                    },
                ),
            ]
            .into_iter()
            .collect();
            assert_eq!(
                r.grid_order(),
                Some(vec![tla("CCC"), tla("AAA"), tla("BBB")])
            );
        }
    }

    mod given_a_sprint_with_a_retirement {
        use super::*;

        #[test]
        fn when_the_sprint_order_is_requested_then_the_retired_car_is_last_and_flagged() {
            let mut r = Round::new(RoundNumber::new(1).unwrap(), "Test");
            r.sprint = [
                (
                    tla("AAA"),
                    RaceRow {
                        grid: 1,
                        pos: 2,
                        ..Default::default()
                    },
                ),
                (
                    tla("BBB"),
                    RaceRow {
                        grid: 2,
                        pos: 3,
                        dnf: true,
                        ..Default::default()
                    },
                ),
                (
                    tla("CCC"),
                    RaceRow {
                        grid: 3,
                        pos: 1,
                        ..Default::default()
                    },
                ),
            ]
            .into_iter()
            .collect();
            let order = r.sprint_order().unwrap();
            assert_eq!(order.finish, vec![tla("CCC"), tla("AAA"), tla("BBB")]);
            assert_eq!(order.grid, vec![tla("AAA"), tla("BBB"), tla("CCC")]);
            assert!(order.dnf.contains(&tla("BBB")));
        }
    }
}
